import * as anchor from "@anchor-lang/core";
import { Program, BN } from "@anchor-lang/core";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";

import {
  getAccount,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import {
  accountExist,
  closeCreatorFeeShare,
  collectCreatorFee,
  collectCreatorFeePermissionless,
  collectSharedCreatorFee,
  createAmmConfig,
  createCreatorFeeShare,
  createPermissionPda,
  createTokenMintAndAssociatedTokenAccount,
  getCreatorFeeShareAddress,
  initializeWithPermission,
  sendTransaction,
  swap_base_input,
  updateAmmConfig,
  retryUntilPoolOpen,
  withdraw,
  getPoolLpMintAddress,
  getPoolVaultAddress,
} from "./utils";
import { assert } from "chai";
import { SystemProgram } from "@solana/web3.js";

const FEE_RATE_DENOMINATOR = new BN(1_000_000);
const CREATOR_FEE_SHARE_RATE_PARAM = 8;

describe("creator fee share test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  const connection = anchor.getProvider().connection;
  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = { skipPreflight: true };

  // Everything here is admin gated: creating an amm config, the permission pda that
  // enables the creator fee, the share account and the admin collect instruction all
  // check `crate::admin::ID`. `yarn test:local-admin` builds with `--features localnet`
  // and compiles the local wallet in as admin; on a stock build the suite is skipped.
  const adminIsLocalWallet =
    process.env.CPSWAP_LOCALNET_ADMIN === owner.publicKey.toBase58();

  // A pool only accrues a creator fee when it was created through
  // `initialize_with_permission`, so the whole suite shares one such pool and every
  // case swaps again to accrue a fresh fee.
  const creator = Keypair.generate();
  const outsider = Keypair.generate();
  let configAddress: PublicKey;
  let poolAddress: PublicKey;
  let token0: PublicKey;
  let token0Program: PublicKey;
  let token1: PublicKey;
  let token1Program: PublicKey;

  before(async function () {
    if (!adminIsLocalWallet) {
      this.skip();
    }
    this.timeout(300000);

    await sendTransaction(connection, [
      SystemProgram.transfer({
        fromPubkey: owner.publicKey,
        toPubkey: creator.publicKey,
        lamports: LAMPORTS_PER_SOL,
      }),
      SystemProgram.transfer({
        fromPubkey: owner.publicKey,
        toPubkey: outsider.publicKey,
        lamports: LAMPORTS_PER_SOL,
      }),
    ], [owner]);

    // 2.5% trade fee and 2.5% creator fee, so a swap accrues a fee large enough for
    // the split to be exact rather than lost to rounding
    configAddress = await createAmmConfig(
      program,
      connection,
      owner,
      880,
      new BN(25000),
      new BN(120000),
      new BN(40000),
      new BN(0),
      new BN(25000),
      confirmOptions
    );

    await createPermissionPda(
      program,
      connection,
      owner,
      owner.publicKey,
      confirmOptions
    );

    const tokens = await createTokenMintAndAssociatedTokenAccount(
      connection,
      owner,
      new Keypair(),
      { transferFeeBasisPoints: 0, MaxFee: 0 }
    );
    token0 = tokens[0].token0;
    token0Program = tokens[0].token0Program;
    token1 = tokens[1].token1;
    token1Program = tokens[1].token1Program;

    const pool = await initializeWithPermission(
      program,
      owner,
      creator.publicKey,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      { bothToken: {} },
      confirmOptions
    );
    poolAddress = pool.poolAddress;
  });

  async function setShareRate(rate: number) {
    await updateAmmConfig(
      program,
      owner,
      configAddress,
      CREATOR_FEE_SHARE_RATE_PARAM,
      new BN(rate),
      confirmOptions
    );
  }

  /// Swapping token_0 in accrues the creator fee on token_0 (`CreatorFeeOn::BothToken`
  /// takes the fee on the input token) and returns the accrued amount.
  async function accrueCreatorFee(): Promise<BN> {
    // no `skipPreflight`, so a rejection arrives as a readable AnchorError rather than
    // being mangled by the provider's error wrapping
    await retryUntilPoolOpen(() =>
      swap_base_input(
        program,
        owner,
        configAddress,
        token0,
        token0Program,
        token1,
        token1Program,
        new BN(100000000),
        new BN(0)
      )
    );
    const poolState = await program.account.poolState.fetch(poolAddress);
    assert.isTrue(
      poolState.creatorFeesToken0.gtn(0),
      "the swap must accrue a creator fee for the split to be observable"
    );
    return poolState.creatorFeesToken0;
  }

  /// The mirror of `accrueCreatorFee` on the other leg: swapping the pool's token_1 in
  /// accrues the creator fee on token_1. The pool pda is derived in token_0/token_1
  /// order, so it has to be passed explicitly here.
  async function accrueCreatorFeeOnToken1(): Promise<BN> {
    await retryUntilPoolOpen(() =>
      swap_base_input(
        program,
        owner,
        configAddress,
        token1,
        token1Program,
        token0,
        token0Program,
        new BN(100000000),
        new BN(0),
        undefined,
        poolAddress
      )
    );
    const poolState = await program.account.poolState.fetch(poolAddress);
    assert.isTrue(
      poolState.creatorFeesToken1.gtn(0),
      "the swap must accrue a creator fee on token_1"
    );
    return poolState.creatorFeesToken1;
  }

  async function tokenBalance(mint: PublicKey, tokenProgram: PublicKey, authority: PublicKey) {
    const address = getAssociatedTokenAddressSync(
      mint,
      authority,
      false,
      tokenProgram
    );
    if (!(await accountExist(connection, address))) {
      return new BN(0);
    }
    const account = await getAccount(
      connection,
      address,
      "processed",
      tokenProgram
    );
    return new BN(account.amount.toString());
  }

  // mirrors `floor_div` on chain: the protocol share rounds down
  function expectedSharedAmount(creatorFee: BN, rate: number) {
    return creatorFee.mul(new BN(rate)).div(FEE_RATE_DENOMINATOR);
  }

  it("a zero config rate and no share account leave the whole fee to the creator", async () => {
    await setShareRate(0);
    const creatorFee = await accrueCreatorFee();
    const creatorBefore = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolBefore = await program.account.poolState.fetch(poolAddress);

    // exercised through the permissionless variant, which now also resolves the split
    await collectCreatorFeePermissionless(
      program,
      owner,
      creator.publicKey,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const creatorAfter = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(creatorAfter.sub(creatorBefore).toString(), creatorFee.toString());
    assert.equal(
      poolAfter.sharedCreatorFeesToken0.toString(),
      poolBefore.sharedCreatorFeesToken0.toString()
    );
    assert.equal(poolAfter.creatorFeesToken0.toString(), "0");
  });

  it("the config rate splits the fee and books the remainder on the pool", async () => {
    await setShareRate(200000);
    const creatorFee = await accrueCreatorFee();
    const sharedAmount = expectedSharedAmount(creatorFee, 200000);
    const creatorBefore = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolBefore = await program.account.poolState.fetch(poolAddress);

    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const creatorAfter = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      creatorAfter.sub(creatorBefore).toString(),
      creatorFee.sub(sharedAmount).toString()
    );
    assert.equal(
      poolAfter.sharedCreatorFeesToken0
        .sub(poolBefore.sharedCreatorFeesToken0)
        .toString(),
      sharedAmount.toString()
    );
    assert.equal(poolAfter.creatorFeesToken0.toString(), "0");
  });

  it("a share account overrides the config rate", async () => {
    const shareAddress = await createCreatorFeeShare(
      program,
      owner,
      creator.publicKey,
      configAddress,
      new BN(500000),
      confirmOptions
    );
    const share = await program.account.creatorFeeShare.fetch(shareAddress);
    assert.equal(share.creator.toBase58(), creator.publicKey.toBase58());
    assert.equal(share.ammConfig.toBase58(), configAddress.toBase58());
    assert.equal(share.shareRate.toString(), "500000");

    // the config still says 20%, the account must win
    const creatorFee = await accrueCreatorFee();
    const sharedAmount = expectedSharedAmount(creatorFee, 500000);
    const creatorBefore = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolBefore = await program.account.poolState.fetch(poolAddress);

    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const creatorAfter = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      creatorAfter.sub(creatorBefore).toString(),
      creatorFee.sub(sharedAmount).toString()
    );
    assert.equal(
      poolAfter.sharedCreatorFeesToken0
        .sub(poolBefore.sharedCreatorFeesToken0)
        .toString(),
      sharedAmount.toString()
    );
  });

  it("closing the share account falls back to the config rate", async () => {
    await closeCreatorFeeShare(
      program,
      owner,
      creator.publicKey,
      configAddress,
      confirmOptions
    );
    const [shareAddress] = await getCreatorFeeShareAddress(
      creator.publicKey,
      configAddress,
      program.programId
    );
    assert.isFalse(await accountExist(connection, shareAddress));

    const creatorFee = await accrueCreatorFee();
    const sharedAmount = expectedSharedAmount(creatorFee, 200000);
    const creatorBefore = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolBefore = await program.account.poolState.fetch(poolAddress);

    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const creatorAfter = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      creatorAfter.sub(creatorBefore).toString(),
      creatorFee.sub(sharedAmount).toString()
    );
    assert.equal(
      poolAfter.sharedCreatorFeesToken0
        .sub(poolBefore.sharedCreatorFeesToken0)
        .toString(),
      sharedAmount.toString()
    );
  });

  it("admin collects the shared creator fee and can take it partially", async () => {
    const poolBefore = await program.account.poolState.fetch(poolAddress);
    const accrued = poolBefore.sharedCreatorFeesToken0;
    assert.isTrue(accrued.gtn(1), "the earlier cases must have booked a shared creator fee");

    const half = accrued.divn(2);
    const ownerBefore = await tokenBalance(token0, token0Program, owner.publicKey);
    await collectSharedCreatorFee(
      program,
      owner,
      poolAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      half,
      new BN(0),
      confirmOptions
    );
    let poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      poolAfter.sharedCreatorFeesToken0.toString(),
      accrued.sub(half).toString()
    );
    assert.equal(
      (await tokenBalance(token0, token0Program, owner.publicKey))
        .sub(ownerBefore)
        .toString(),
      half.toString()
    );

    // a request larger than the balance is capped at the balance
    const rest = poolAfter.sharedCreatorFeesToken0;
    await collectSharedCreatorFee(
      program,
      owner,
      poolAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      new BN("18446744073709551615"),
      new BN("18446744073709551615"),
      confirmOptions
    );
    poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(poolAfter.sharedCreatorFeesToken0.toString(), "0");
    assert.equal(
      (await tokenBalance(token0, token0Program, owner.publicKey))
        .sub(ownerBefore)
        .toString(),
      half.add(rest).toString()
    );
  });

  it("collecting an empty shared creator fee fails", async () => {
    try {
      await collectSharedCreatorFee(
        program,
        owner,
        poolAddress,
        token0,
        token0Program,
        token1,
        token1Program,
        new BN(1000),
        new BN(1000)
      );
      assert.fail("collecting nothing must be rejected");
    } catch (e) {
      assert.include(String(e), "NoFeeCollect");
    }
  });

  it("a share rate above the fee denominator is rejected", async () => {
    try {
      await createCreatorFeeShare(
        program,
        owner,
        outsider.publicKey,
        configAddress,
        FEE_RATE_DENOMINATOR.addn(1)
      );
      assert.fail("an out of range share rate must be rejected");
    } catch (e) {
      assert.include(String(e), "InvalidInput");
    }
  });

  it("a non admin cannot create or close a share account", async () => {
    try {
      await createCreatorFeeShare(
        program,
        outsider,
        creator.publicKey,
        configAddress,
        new BN(500000)
      );
      assert.fail("a non admin must not create a share account");
    } catch (e) {
      assert.include(String(e), "InvalidOwner");
    }

    const shareAddress = await createCreatorFeeShare(
      program,
      owner,
      creator.publicKey,
      configAddress,
      new BN(500000),
      confirmOptions
    );
    try {
      await closeCreatorFeeShare(
        program,
        outsider,
        creator.publicKey,
        configAddress
      );
      assert.fail("a non admin must not close a share account");
    } catch (e) {
      assert.include(String(e), "InvalidOwner");
    }
    assert.isTrue(await accountExist(connection, shareAddress));
    await closeCreatorFeeShare(
      program,
      owner,
      creator.publicKey,
      configAddress,
      confirmOptions
    );
  });

  it("a non admin cannot collect the shared creator fee", async () => {
    await setShareRate(200000);
    await accrueCreatorFee();
    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );
    const poolState = await program.account.poolState.fetch(poolAddress);
    assert.isTrue(poolState.sharedCreatorFeesToken0.gtn(0));

    try {
      // recipients are the admin's own token accounts, so the only thing that can
      // fail is the owner check
      await collectSharedCreatorFee(
        program,
        outsider,
        poolAddress,
        token0,
        token0Program,
        token1,
        token1Program,
        new BN("18446744073709551615"),
        new BN("18446744073709551615"),
        undefined,
        owner.publicKey
      );
      assert.fail("a non admin must not collect the shared creator fee");
    } catch (e) {
      assert.include(String(e), "InvalidOwner");
    }
  });
  it("splits and collects the fee accrued on token_1 as well", async () => {
    await setShareRate(200000);
    const creatorFee = await accrueCreatorFeeOnToken1();
    const sharedAmount = expectedSharedAmount(creatorFee, 200000);
    assert.isTrue(
      sharedAmount.gtn(0),
      "the token_1 share must be non zero for this to prove anything"
    );
    const creatorBefore = await tokenBalance(token1, token1Program, creator.publicKey);
    const poolBefore = await program.account.poolState.fetch(poolAddress);

    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const creatorAfter = await tokenBalance(token1, token1Program, creator.publicKey);
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      creatorAfter.sub(creatorBefore).toString(),
      creatorFee.sub(sharedAmount).toString()
    );
    assert.equal(
      poolAfter.sharedCreatorFeesToken1
        .sub(poolBefore.sharedCreatorFeesToken1)
        .toString(),
      sharedAmount.toString()
    );
    assert.equal(poolAfter.creatorFeesToken1.toString(), "0");

    // and the admin can take the token_1 side out of the vault
    const adminBefore = await tokenBalance(token1, token1Program, owner.publicKey);
    const booked = poolAfter.sharedCreatorFeesToken1;
    await collectSharedCreatorFee(
      program,
      owner,
      poolAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      new BN(0),
      booked,
      confirmOptions
    );
    const adminAfter = await tokenBalance(token1, token1Program, owner.publicKey);
    assert.equal(adminAfter.sub(adminBefore).toString(), booked.toString());
    const poolFinal = await program.account.poolState.fetch(poolAddress);
    assert.equal(poolFinal.sharedCreatorFeesToken1.toString(), "0");
  });

  it("applies the rate active at collection to the whole accrued balance", async () => {
    // accrue under one rate...
    await setShareRate(100000);
    const firstFee = await accrueCreatorFeeOnToken1();

    // ...then change it before collecting; the split must use the new rate for the
    // whole batch rather than blending the two
    await setShareRate(400000);
    const totalFee = await accrueCreatorFeeOnToken1();
    assert.isTrue(
      totalFee.gt(firstFee),
      "the second swap must add to the already accrued fee"
    );

    const atCollectionRate = expectedSharedAmount(totalFee, 400000);
    const blended = expectedSharedAmount(firstFee, 100000).add(
      expectedSharedAmount(totalFee.sub(firstFee), 400000)
    );
    assert.isFalse(
      atCollectionRate.eq(blended),
      "the two interpretations must differ for this case to be meaningful"
    );

    const poolBefore = await program.account.poolState.fetch(poolAddress);
    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      poolAfter.sharedCreatorFeesToken1
        .sub(poolBefore.sharedCreatorFeesToken1)
        .toString(),
      atCollectionRate.toString()
    );
  });

  it("a full share rate leaves the creator nothing and still books the whole fee", async () => {
    await setShareRate(FEE_RATE_DENOMINATOR.toNumber());
    const creatorFee = await accrueCreatorFee();
    const creatorBefore = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolBefore = await program.account.poolState.fetch(poolAddress);

    // the creator leg transfers zero, which short circuits inside the transfer helper
    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const creatorAfter = await tokenBalance(token0, token0Program, creator.publicKey);
    const poolAfter = await program.account.poolState.fetch(poolAddress);
    assert.equal(creatorAfter.sub(creatorBefore).toString(), "0");
    assert.equal(
      poolAfter.sharedCreatorFeesToken0
        .sub(poolBefore.sharedCreatorFeesToken0)
        .toString(),
      creatorFee.toString()
    );
  });

  it("rejects a config share rate above the fee denominator", async () => {
    // the handler asserts, so this surfaces as a program panic rather than an error code
    let rejected = false;
    try {
      await updateAmmConfig(
        program,
        owner,
        configAddress,
        CREATOR_FEE_SHARE_RATE_PARAM,
        FEE_RATE_DENOMINATOR.addn(1)
      );
    } catch {
      rejected = true;
    }
    assert.isTrue(rejected, "an out of range config share rate must be rejected");
    const ammConfig = await program.account.ammConfig.fetch(configAddress);
    assert.isTrue(ammConfig.creatorFeeShareRate.lte(FEE_RATE_DENOMINATOR));
  });

  // last: this one withdraws the pool's liquidity, so it must not run before the cases
  // above that need a pool to swap against
  it("an LP cannot withdraw the shared creator fee held in the vault", async () => {
    await setShareRate(200000);
    await accrueCreatorFee();
    await collectCreatorFee(
      program,
      creator,
      poolAddress,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );

    const poolBefore = await program.account.poolState.fetch(poolAddress);
    const booked0 = poolBefore.sharedCreatorFeesToken0;
    assert.isTrue(booked0.gtn(0), "a shared creator fee must be booked on the pool");
    // everything the pool owes to someone other than the LPs
    const owed0 = booked0
      .add(poolBefore.protocolFeesToken0)
      .add(poolBefore.fundFeesToken0)
      .add(poolBefore.creatorFeesToken0);

    // the LP burns every lp token it holds
    const [lpMint] = await getPoolLpMintAddress(poolAddress, program.programId);
    const lpTokenAmount = await tokenBalance(
      lpMint,
      TOKEN_PROGRAM_ID,
      owner.publicKey
    );
    assert.isTrue(lpTokenAmount.gtn(0), "the owner must hold lp tokens to burn");
    await withdraw(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      lpTokenAmount,
      new BN(0),
      new BN(0),
      confirmOptions
    );

    // the vault still holds everything that was booked, so the LP could not take it
    const [vault0] = await getPoolVaultAddress(
      poolAddress,
      token0,
      program.programId
    );
    const vaultAfter = new BN(
      (
        await getAccount(connection, vault0, "processed", token0Program)
      ).amount.toString()
    );
    assert.isTrue(
      vaultAfter.gte(owed0),
      `vault holds ${vaultAfter.toString()} but owes ${owed0.toString()}`
    );

    // and the admin can still take the shared creator fee out
    const adminBefore = await tokenBalance(token0, token0Program, owner.publicKey);
    await collectSharedCreatorFee(
      program,
      owner,
      poolAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      booked0,
      new BN(0),
      confirmOptions
    );
    const adminAfter = await tokenBalance(token0, token0Program, owner.publicKey);
    assert.equal(adminAfter.sub(adminBefore).toString(), booked0.toString());
  });
});
