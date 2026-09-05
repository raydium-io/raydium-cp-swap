import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";

import {
  TOKEN_PROGRAM_ID,
  createWrappedNativeAccount,
  getAccount,
} from "@solana/spl-token";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  SystemProgram,
  PublicKey,
} from "@solana/web3.js";
import {
  setupInitializeTest,
  initialize,
  collectExcessLamports,
  sendTransaction,
  ensureMainnetTokenProgram,
  getAuthAddress,
} from "./utils";
import { assert } from "chai";

describe("collect excess lamports test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  console.log("owner: ", owner.publicKey.toString());

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  // collect_excess_lamports gates its signer on crate::admin::ID /
  // crate::collect_lamports::ID — production keys on a stock build.
  // `yarn test:local-admin` builds with `--features localnet` and compiles the
  // local wallet in as admin so the admin cases below can sign as admin;
  // otherwise they are skipped.
  const adminIsLocalWallet =
    process.env.CPSWAP_LOCALNET_ADMIN === owner.publicKey.toBase58();

  // the legacy-token cases need the mainnet token program (p-token); replace
  // the surfnet's bundled build before any transaction runs (this file is
  // alphabetically first), a mid-suite swap leaves the surfnet inconsistent
  let mainnetTokenProgram = false;
  before(async () => {
    mainnetTokenProgram = await ensureMainnetTokenProgram(
      anchor.getProvider().connection
    );
  });

  async function setupPool() {
    const { configAddress, token0, token0Program, token1, token1Program } =
      await setupInitializeTest(
        program,
        anchor.getProvider().connection,
        owner,
        {
          config_index: 0,
          tradeFeeRate: new BN(10),
          protocolFeeRate: new BN(1000),
          fundFeeRate: new BN(25000),
          create_fee: new BN(0),
        },
        { transferFeeBasisPoints: 0, MaxFee: 0 },
        confirmOptions
      );

    return await initialize(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions
    );
  }

  async function donateLamports(to: PublicKey, lamports: number) {
    await sendTransaction(
      anchor.getProvider().connection,
      [
        SystemProgram.transfer({
          fromPubkey: owner.publicKey,
          toPubkey: to,
          lamports,
        }),
      ],
      [owner],
      confirmOptions
    );
  }

  async function getRentMinimumBalance(address: PublicKey) {
    const info = await anchor.getProvider().connection.getAccountInfo(address);
    return await anchor
      .getProvider()
      .connection.getMinimumBalanceForRentExemption(info.data.length);
  }

  it("collect excess lamports from token2022 vault and pool state", async function () {
    if (!adminIsLocalWallet) this.skip();

    const { poolAddress, poolState } = await setupPool();
    // the default setup creates one legacy token mint and one token2022 mint
    const vault2022 = poolState.token0Program.equals(TOKEN_PROGRAM_ID)
      ? poolState.token1Vault
      : poolState.token0Vault;

    await donateLamports(vault2022, LAMPORTS_PER_SOL / 2);
    await donateLamports(poolAddress, LAMPORTS_PER_SOL / 4);

    const ownerBalanceBefore = await anchor
      .getProvider()
      .connection.getBalance(owner.publicKey);

    await collectExcessLamports(
      program,
      owner,
      [vault2022, poolAddress],
      confirmOptions
    );

    assert.equal(
      await anchor.getProvider().connection.getBalance(vault2022),
      await getRentMinimumBalance(vault2022)
    );
    assert.equal(
      await anchor.getProvider().connection.getBalance(poolAddress),
      await getRentMinimumBalance(poolAddress)
    );
    const ownerBalanceAfter = await anchor
      .getProvider()
      .connection.getBalance(owner.publicKey);
    assert(ownerBalanceAfter > ownerBalanceBefore);
  });

  it("collect excess lamports, account with zero excess does not stop processing later accounts", async function () {
    if (!adminIsLocalWallet) this.skip();

    const { poolAddress, poolState } = await setupPool();

    // observation state is freshly created, i.e. exactly at the rent-exempt
    // minimum; it must be skipped without cutting off the rest of the list
    await donateLamports(poolAddress, LAMPORTS_PER_SOL / 5);

    await collectExcessLamports(
      program,
      owner,
      [poolState.observationKey, poolAddress],
      confirmOptions
    );

    assert.equal(
      await anchor.getProvider().connection.getBalance(poolAddress),
      await getRentMinimumBalance(poolAddress)
    );
  });

  it("collect excess lamports with invalid owner", async () => {
    const { poolAddress } = await setupPool();
    await donateLamports(poolAddress, LAMPORTS_PER_SOL / 10);

    const invalidOwner = Keypair.generate();
    await donateLamports(invalidOwner.publicKey, LAMPORTS_PER_SOL);

    try {
      await collectExcessLamports(program, invalidOwner, [poolAddress]);
      assert.fail("expected InvalidOwner");
    } catch (err) {
      assert.include(err.toString(), "InvalidOwner");
    }
  });

  it("collect excess lamports from legacy token vault", async function () {
    // the mainnet legacy token program (p-token) supports
    // WithdrawExcessLamports, the build bundled with local validators does not
    if (!adminIsLocalWallet || !mainnetTokenProgram) this.skip();

    const { poolState } = await setupPool();
    const vaultLegacy = poolState.token0Program.equals(TOKEN_PROGRAM_ID)
      ? poolState.token0Vault
      : poolState.token1Vault;

    await donateLamports(vaultLegacy, LAMPORTS_PER_SOL / 10);

    await collectExcessLamports(program, owner, [vaultLegacy], confirmOptions);

    assert.equal(
      await anchor.getProvider().connection.getBalance(vaultLegacy),
      await getRentMinimumBalance(vaultLegacy)
    );
  });

  it("collect excess lamports from a native (WSOL) account, wrapped balance unchanged", async function () {
    if (!adminIsLocalWallet || !mainnetTokenProgram) this.skip();

    const connection = anchor.getProvider().connection;
    const [authority] = await getAuthAddress(program.programId);
    // pass a keypair so the account is a plain (non-associated) token account,
    // the associated path cannot derive an address for an off-curve owner.
    // wrap 1 SOL, so the account has a real wrapped balance to preserve.
    const wrappedAmount = LAMPORTS_PER_SOL;
    const wsolAccount = await createWrappedNativeAccount(
      connection,
      owner,
      authority,
      wrappedAmount,
      Keypair.generate(),
      confirmOptions
    );

    // donate raw lamports on top of the rent + wrapped balance
    const donation = LAMPORTS_PER_SOL / 10;
    await donateLamports(wsolAccount, donation);

    // a native account cannot use WithdrawExcessLamports; the program folds the
    // donation into the wrapped amount via SyncNative, then UnwrapLamports pulls
    // exactly the donation back out, leaving the wrapped balance untouched
    await collectExcessLamports(program, owner, [wsolAccount], confirmOptions);

    // only the donation is removed; a native account keeps rent + wrapped amount
    assert.equal(
      await connection.getBalance(wsolAccount),
      (await getRentMinimumBalance(wsolAccount)) + wrappedAmount
    );
    // the wrapped token balance is exactly what it was before
    const wsol = await getAccount(
      connection,
      wsolAccount,
      "processed",
      TOKEN_PROGRAM_ID
    );
    assert.equal(wsol.amount.toString(), wrappedAmount.toString());
  });
});
