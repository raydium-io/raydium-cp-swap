import * as anchor from "@anchor-lang/core";
import { Program, BN } from "@anchor-lang/core";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";

import { getAccount, getAssociatedTokenAddressSync } from "@solana/spl-token";
import { Keypair, PublicKey } from "@solana/web3.js";
import {
  collectCreatorFeePermissionless,
  createAmmConfig,
  createTokenMintAndAssociatedTokenAccount,
  initialize,
  initializeV2,
  retryUntilPoolOpen,
  swap_base_input,
} from "./utils";
import { assert } from "chai";

// `CreatorFeeOn` as stored on the pool: 0 both tokens, 1 only token0, 2 only token1
const CREATOR_FEE_ON_BOTH = 0;
const CREATOR_FEE_ON_TOKEN0 = 1;
const CREATOR_FEE_ON_TOKEN1 = 2;

type Pool = {
  poolAddress: PublicKey;
  token0: PublicKey;
  token0Program: PublicKey;
  token1: PublicKey;
  token1Program: PublicKey;
};

describe("initialize_v2 test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  const connection = anchor.getProvider().connection;
  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = { skipPreflight: true };

  // `initialize_v2` itself is permissionless, but the suite needs an amm config with a
  // known creator fee rate, and creating one is admin gated; `yarn test:local-admin`
  // compiles the local wallet in as admin, on a stock build the suite is skipped.
  const adminIsLocalWallet =
    process.env.CPSWAP_LOCALNET_ADMIN === owner.publicKey.toBase58();

  let configAddress: PublicKey;
  // the first pool created through `initialize_v2`, reused by the later cases
  let v2Pool: Pool;

  before(async function () {
    if (!adminIsLocalWallet) {
      this.skip();
    }
    this.timeout(300000);

    // 2.5% trade fee and 2.5% creator fee, so every swap accrues a visible creator fee
    configAddress = await createAmmConfig(
      program,
      connection,
      owner,
      881,
      new BN(25000),
      new BN(120000),
      new BN(40000),
      new BN(0),
      new BN(25000),
      confirmOptions
    );
  });

  async function newTokenPair() {
    const [{ token0, token0Program }, { token1, token1Program }] =
      await createTokenMintAndAssociatedTokenAccount(
        connection,
        owner,
        new Keypair(),
        { transferFeeBasisPoints: 0, MaxFee: 0 }
      );
    return { token0, token0Program, token1, token1Program };
  }

  /// Swaps `input` into the pool and returns the creator fees accrued on the pool.
  async function swapAndReadCreatorFees(
    pool: Pool,
    input: "token0" | "token1"
  ) {
    const zeroForOne = input === "token0";
    await retryUntilPoolOpen(() =>
      swap_base_input(
        program,
        owner,
        configAddress,
        zeroForOne ? pool.token0 : pool.token1,
        zeroForOne ? pool.token0Program : pool.token1Program,
        zeroForOne ? pool.token1 : pool.token0,
        zeroForOne ? pool.token1Program : pool.token0Program,
        new BN(100000000),
        new BN(0),
        undefined,
        // the pool pda is derived in token_0/token_1 order, so it has to be passed
        // explicitly when token_1 is the input
        pool.poolAddress
      )
    );
    const poolState = await program.account.poolState.fetch(pool.poolAddress);
    return {
      token0: poolState.creatorFeesToken0,
      token1: poolState.creatorFeesToken1,
    };
  }

  async function tokenBalance(mint: PublicKey, tokenProgram: PublicKey) {
    const address = getAssociatedTokenAddressSync(
      mint,
      owner.publicKey,
      false,
      tokenProgram
    );
    const account = await getAccount(
      connection,
      address,
      "processed",
      tokenProgram
    );
    return new BN(account.amount.toString());
  }

  it("creates a pool with the creator fee on and charges it only on the chosen token", async () => {
    const pair = await newTokenPair();
    const { poolAddress, poolState } = await initializeV2(
      program,
      owner,
      configAddress,
      pair.token0,
      pair.token0Program,
      pair.token1,
      pair.token1Program,
      { onlyToken1: {} },
      confirmOptions
    );
    assert.isTrue(poolState.enableCreatorFee);
    assert.equal(poolState.creatorFeeOn, CREATOR_FEE_ON_TOKEN1);
    assert.isTrue(
      poolState.poolCreator.equals(owner.publicKey),
      "the signer is recorded as the pool creator"
    );
    v2Pool = { poolAddress, ...pair };

    // token_0 in: `OnlyToken1` takes the fee on the output leg
    let fees = await swapAndReadCreatorFees(v2Pool, "token0");
    assert.equal(fees.token0.toString(), "0");
    assert.isTrue(fees.token1.gtn(0), "the creator fee must accrue on token_1");
    const afterFirstSwap = fees.token1;

    // token_1 in: the fee stays on token_1, now taken on the input leg
    fees = await swapAndReadCreatorFees(v2Pool, "token1");
    assert.equal(fees.token0.toString(), "0");
    assert.isTrue(fees.token1.gt(afterFirstSwap));
  });

  it("charges only token_0 when asked", async () => {
    const pair = await newTokenPair();
    const { poolAddress, poolState } = await initializeV2(
      program,
      owner,
      configAddress,
      pair.token0,
      pair.token0Program,
      pair.token1,
      pair.token1Program,
      { onlyToken0: {} },
      confirmOptions
    );
    assert.isTrue(poolState.enableCreatorFee);
    assert.equal(poolState.creatorFeeOn, CREATOR_FEE_ON_TOKEN0);
    const pool = { poolAddress, ...pair };

    // token_1 in: the fee is taken on the output leg, token_0
    let fees = await swapAndReadCreatorFees(pool, "token1");
    assert.isTrue(fees.token0.gtn(0), "the creator fee must accrue on token_0");
    assert.equal(fees.token1.toString(), "0");
    const afterFirstSwap = fees.token0;

    // token_0 in: still token_0, now on the input leg
    fees = await swapAndReadCreatorFees(pool, "token0");
    assert.isTrue(fees.token0.gt(afterFirstSwap));
    assert.equal(fees.token1.toString(), "0");
  });

  it("charges the input token when both tokens are allowed", async () => {
    const pair = await newTokenPair();
    const { poolAddress, poolState } = await initializeV2(
      program,
      owner,
      configAddress,
      pair.token0,
      pair.token0Program,
      pair.token1,
      pair.token1Program,
      { bothToken: {} },
      confirmOptions
    );
    assert.isTrue(poolState.enableCreatorFee);
    assert.equal(poolState.creatorFeeOn, CREATOR_FEE_ON_BOTH);
    const pool = { poolAddress, ...pair };

    let fees = await swapAndReadCreatorFees(pool, "token0");
    assert.isTrue(fees.token0.gtn(0));
    assert.equal(fees.token1.toString(), "0");

    fees = await swapAndReadCreatorFees(pool, "token1");
    assert.isTrue(fees.token1.gtn(0));
  });

  it("stores the creator fee as disabled when no fee model is given", async () => {
    const pair = await newTokenPair();
    const { poolAddress, poolState } = await initializeV2(
      program,
      owner,
      configAddress,
      pair.token0,
      pair.token0Program,
      pair.token1,
      pair.token1Program,
      null,
      confirmOptions
    );
    assert.isFalse(poolState.enableCreatorFee);
    // stored exactly like a pool created through `initialize`
    assert.equal(poolState.creatorFeeOn, CREATOR_FEE_ON_BOTH);

    const pool = { poolAddress, ...pair };
    let fees = await swapAndReadCreatorFees(pool, "token0");
    assert.equal(fees.token0.toString(), "0");
    assert.equal(fees.token1.toString(), "0");
    fees = await swapAndReadCreatorFees(pool, "token1");
    assert.equal(fees.token0.toString(), "0");
    assert.equal(fees.token1.toString(), "0");
  });

  it("the plain initialize keeps the creator fee off on the same config", async () => {
    const pair = await newTokenPair();
    const { poolAddress, poolState } = await initialize(
      program,
      owner,
      configAddress,
      pair.token0,
      pair.token0Program,
      pair.token1,
      pair.token1Program,
      confirmOptions
    );
    assert.isFalse(poolState.enableCreatorFee);
    assert.equal(poolState.creatorFeeOn, CREATOR_FEE_ON_BOTH);

    const fees = await swapAndReadCreatorFees(
      { poolAddress, ...pair },
      "token0"
    );
    assert.equal(fees.token0.toString(), "0");
    assert.equal(fees.token1.toString(), "0");
  });

  it("the creator collects the fee accrued by a v2 pool", async () => {
    const poolBefore = await program.account.poolState.fetch(
      v2Pool.poolAddress
    );
    assert.isTrue(poolBefore.creatorFeesToken1.gtn(0));
    const balanceBefore = await tokenBalance(
      v2Pool.token1,
      v2Pool.token1Program
    );

    await collectCreatorFeePermissionless(
      program,
      owner,
      owner.publicKey,
      v2Pool.poolAddress,
      configAddress,
      v2Pool.token0,
      v2Pool.token0Program,
      v2Pool.token1,
      v2Pool.token1Program,
      confirmOptions
    );

    const balanceAfter = await tokenBalance(
      v2Pool.token1,
      v2Pool.token1Program
    );
    // the config's creator fee share rate is 0, so the whole fee goes to the creator
    assert.equal(
      balanceAfter.sub(balanceBefore).toString(),
      poolBefore.creatorFeesToken1.toString()
    );
    const poolAfter = await program.account.poolState.fetch(v2Pool.poolAddress);
    assert.equal(poolAfter.creatorFeesToken1.toString(), "0");
  });
});
