import * as anchor from "@anchor-lang/core";
import { Program, BN } from "@anchor-lang/core";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  ExtensionType,
  TOKEN_2022_PROGRAM_ID,
  createInitializeMintInstruction,
  createInitializeTransferHookInstruction,
  getMintLen,
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
  createMintToInstruction,
  createSetAuthorityInstruction,
  AuthorityType,
} from "@solana/spl-token";
import {
  setupInitializeTest,
  initialize,
  swap_base_input,
  swap_base_output,
  deposit,
  withdraw,
  installStubHookProgram,
  createExtraAccountMetaList,
  metaLiteral,
  metaSeededByAmount,
  metaSeededByAccountData,
  metaReadFromAccountData,
  getAuthAddress,
  waitUntilPoolOpen,
  getPoolAddress,
  getOrcleAccountAddress,
  getPoolVaultAddress,
  initializeWithPermission,
  createCreatorFeeShare,
  createPermissionPda,
} from "./utils";
import { assert } from "chai";

const STUB_ID = new PublicKey("6s7wXukDLKt39UKgbQVExCpwrAsRraEB8jkV6P95ZcSR");

/// Rates out of 1_000_000. Low enough that pool-admission tests never trip over fees.
const LOW_FEES = {
  tradeFeeRate: new BN(10),
  protocolFeeRate: new BN(1000),
  fundFeeRate: new BN(25000),
  create_fee: new BN(0),
  creatorFeeRate: new BN(0),
};
/// High enough that one modest swap leaves every fee bucket non-zero.
const COLLECTABLE_FEES = {
  tradeFeeRate: new BN(10_000),
  protocolFeeRate: new BN(200_000),
  fundFeeRate: new BN(100_000),
  creatorFeeRate: new BN(10_000),
};

describe("transfer hook mint admission", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  const connection = anchor.getProvider().connection;
  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;
  const confirmOptions = { skipPreflight: false };

  before(async () => {
    await installStubHookProgram(
      connection,
      STUB_ID,
      "target/deploy/transfer_hook_stub.so",
      owner.publicKey
    );
    // Lets `owner` call `initialize_with_permission`. The localnet admin is `owner` too.
    // Another suite may already have created it for the same wallet.
    await createPermissionPda(
      program,
      connection,
      owner,
      owner.publicKey,
      confirmOptions
    );
  });

  /// Admission: without this PDA a hook mint can never create a pool.
  async function whitelist(mint: PublicKey): Promise<PublicKey> {
    const [supportMint] = PublicKey.findProgramAddressSync(
      [Buffer.from("support_mint"), mint.toBuffer()],
      program.programId
    );
    await program.methods
      .createSupportMintAssociated()
      .accountsPartial({
        owner: owner.publicKey,
        tokenMint: mint,
        supportMintAssociated: supportMint,
        systemProgram: SystemProgram.programId,
      })
      .signers([owner])
      .rpc(confirmOptions);
    return supportMint;
  }

  /// A token-2022 mint that carries the `TransferHook` extension. `hookProgram` of `null`
  /// leaves `program_id` unset, which is a legal state and means token-2022 performs no CPI.
  async function createMintWithHookExtension(
    hookProgram: PublicKey | null
  ): Promise<PublicKey> {
    const mintKeypair = Keypair.generate();
    const space = getMintLen([ExtensionType.TransferHook]);
    const lamports = await connection.getMinimumBalanceForRentExemption(space);
    const tx = new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: owner.publicKey,
        newAccountPubkey: mintKeypair.publicKey,
        space,
        lamports,
        programId: TOKEN_2022_PROGRAM_ID,
      }),
      // Both fields are `OptionalNonZeroPubkey`, so all-zeros is how "unset" is encoded.
      // token-2022 refuses to initialize the extension with *both* unset, so the
      // "no hook program" mint starts out with an authority; dropping it afterwards with
      // `SetAuthority` is what makes the extension inert.
      createInitializeTransferHookInstruction(
        mintKeypair.publicKey,
        owner.publicKey,
        hookProgram ?? PublicKey.default,
        TOKEN_2022_PROGRAM_ID
      ),
      createInitializeMintInstruction(
        mintKeypair.publicKey,
        9,
        owner.publicKey,
        null,
        TOKEN_2022_PROGRAM_ID
      )
    );
    await sendAndConfirmTransaction(connection, tx, [owner, mintKeypair]);
    return mintKeypair.publicKey;
  }

  /// The creator's token account has to exist and hold a balance before `initialize` can look
  /// at it, or the run fails on account resolution before reaching any hook logic.
  async function fundMint(mint: PublicKey, amount: number) {
    const ata = getAssociatedTokenAddressSync(
      mint,
      owner.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey,
          ata,
          owner.publicKey,
          mint,
          TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(
          mint,
          ata,
          owner.publicKey,
          amount,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );
  }

  const asMetas = (keys: PublicKey[]) =>
    keys.map((pubkey) => ({ pubkey, isSigner: false, isWritable: false }));

  /// A fresh config, and `mint` (funded) paired with a plain mint so only the mint under test
  /// decides. `pair` is sorted, since the instruction helpers derive the pool PDA from the
  /// mints in the order given; spread it straight into them.
  async function setupPair(
    mint: PublicKey,
    configIndex: number,
    rates: Partial<typeof LOW_FEES> = {}
  ) {
    const { configAddress, token1, token1Program } = await setupInitializeTest(
      program,
      connection,
      owner,
      { config_index: configIndex, ...LOW_FEES, ...rates },
      { transferFeeBasisPoints: 0, MaxFee: 0 },
      confirmOptions
    );
    await fundMint(mint, 1_000_000_000);

    const mintFirst = mint.toBuffer().compare(token1.toBuffer()) < 0;
    const pair = mintFirst
      ? ([
          configAddress,
          mint,
          TOKEN_2022_PROGRAM_ID,
          token1,
          token1Program,
        ] as const)
      : ([
          configAddress,
          token1,
          token1Program,
          mint,
          TOKEN_2022_PROGRAM_ID,
        ] as const);
    const [poolAddress] = await getPoolAddress(
      configAddress,
      pair[1],
      pair[3],
      program.programId
    );
    return { pair, poolAddress };
  }

  async function initializeWith(
    mint: PublicKey,
    configIndex: number,
    extraAccounts: PublicKey[] = []
  ) {
    const { pair } = await setupPair(mint, configIndex);
    return await initialize(
      program,
      owner,
      ...pair,
      confirmOptions,
      { initAmount0: new BN(1000), initAmount1: new BN(1000) },
      undefined,
      asMetas(extraAccounts)
    );
  }

  /// An open pool of a whitelisted stub-hook mint and a plain mint. The hook declares
  /// `extras(pool)` as literal extra accounts; it gets the pool's addresses because a hook can
  /// name them before the pool exists. `hookAccounts` is what every transfer must carry.
  /// `withPermission` creates it through `initialize_with_permission`, the only entry point
  /// that turns the creator fee on.
  async function createHookPool(
    configIndex: number,
    extras: (pool: {
      poolAddress: PublicKey;
      observation: PublicKey;
    }) => PublicKey[] = () => [],
    opts: { rates?: Partial<typeof LOW_FEES>; withPermission?: boolean } = {}
  ) {
    const mint = await createMintWithHookExtension(STUB_ID);
    const supportMint = await whitelist(mint);
    const { pair, poolAddress } = await setupPair(
      mint,
      configIndex,
      opts.rates
    );
    const [observation] = await getOrcleAccountAddress(
      poolAddress,
      program.programId
    );
    const extraKeys = extras({ poolAddress, observation });
    const list = await createExtraAccountMetaList(
      connection,
      owner,
      mint,
      STUB_ID,
      extraKeys.map(metaLiteral)
    );
    const hookAccounts = [STUB_ID, list, ...extraKeys];

    const initAmount = {
      initAmount0: new BN(100_000),
      initAmount1: new BN(100_000),
    };
    const remaining = asMetas([supportMint, ...hookAccounts]);
    if (opts.withPermission) {
      await initializeWithPermission(
        program,
        owner,
        owner.publicKey,
        ...pair,
        undefined,
        confirmOptions,
        initAmount,
        undefined,
        remaining
      );
    } else {
      await initialize(
        program,
        owner,
        ...pair,
        confirmOptions,
        initAmount,
        undefined,
        remaining
      );
    }
    const { openTime } = await program.account.poolState.fetch(poolAddress);
    await waitUntilPoolOpen(connection, owner, openTime.toNumber());
    return { pair, poolAddress, list, hookAccounts };
  }

  /// One swap each way, so both sides of the pool accrue fees whichever side the hook mint
  /// sorted to. `swap_base_input` derives the pool from the order given, so the reverse leg
  /// goes through `swap_base_output`, which takes the pool address.
  async function tradeBothWays(
    pair: Awaited<ReturnType<typeof createHookPool>>["pair"],
    poolAddress: PublicKey,
    hookBag: ReturnType<typeof asMetas>
  ) {
    const [config, mint0, program0, mint1, program1] = pair;
    await swap_base_input(
      program,
      owner,
      ...pair,
      new BN(10_000),
      new BN(0),
      confirmOptions,
      undefined,
      hookBag
    );
    await swap_base_output(
      program,
      owner,
      config,
      mint1,
      program1,
      mint0,
      program0,
      new BN(5_000),
      new BN(100_000),
      confirmOptions,
      poolAddress,
      hookBag
    );
  }

  /// The accounts every collect instruction shares, for the pool behind `pair`.
  async function collectAccounts(
    pair: Awaited<ReturnType<typeof createHookPool>>["pair"],
    poolAddress: PublicKey
  ) {
    const [config, mint0, program0, mint1, program1] = pair;
    const [token0Vault] = await getPoolVaultAddress(
      poolAddress,
      mint0,
      program.programId
    );
    const [token1Vault] = await getPoolVaultAddress(
      poolAddress,
      mint1,
      program.programId
    );
    return {
      poolState: poolAddress,
      ammConfig: config,
      token0Vault,
      token1Vault,
      vault0Mint: mint0,
      vault1Mint: mint1,
      ownerToken0: getAssociatedTokenAddressSync(
        mint0,
        owner.publicKey,
        false,
        program0
      ),
      ownerToken1: getAssociatedTokenAddressSync(
        mint1,
        owner.publicKey,
        false,
        program1
      ),
      token0Program: program0,
      token1Program: program1,
    };
  }

  /// Run `collect` and check it paid out exactly the pool's recorded `fee` on both sides,
  /// leaving nothing behind. Both sides must have accrued something, or the hook side's
  /// transfer would be skipped as a zero amount and the hook never exercised.
  async function expectCollected(
    poolAddress: PublicKey,
    fee: "protocolFees" | "fundFees" | "creatorFees",
    recipients: [PublicKey, PublicKey],
    collect: () => Promise<unknown>,
    // Out of 1_000_000; only the creator fee is split, the rest stays with the protocol.
    shareRate = 0n
  ) {
    const balance = async (ata: PublicKey) =>
      BigInt((await connection.getTokenAccountBalance(ata)).value.amount);
    const before = await program.account.poolState.fetch(poolAddress);
    const owed = [before[`${fee}Token0`], before[`${fee}Token1`]].map((v: BN) =>
      BigInt(v.toString())
    );
    assert.isTrue(
      owed.every((v) => v > 0n),
      `${fee} did not accrue on both sides: ${owed}`
    );
    const balancesBefore = await Promise.all(recipients.map(balance));

    await collect();

    const after = await program.account.poolState.fetch(poolAddress);
    assert.equal(after[`${fee}Token0`].toString(), "0");
    assert.equal(after[`${fee}Token1`].toString(), "0");
    const shared = owed.map((v) => (v * shareRate) / 1_000_000n);
    if (fee === "creatorFees") {
      const protocolGrowth = [0, 1].map(
        (i) =>
          BigInt(after[`protocolFeesToken${i}`].toString()) -
          BigInt(before[`protocolFeesToken${i}`].toString())
      );
      assert.deepEqual(
        protocolGrowth,
        shared,
        "the protocol share was not booked"
      );
    }
    const balancesAfter = await Promise.all(recipients.map(balance));
    assert.deepEqual(
      balancesAfter.map((b, i) => b - balancesBefore[i]),
      owed.map((v, i) => v - shared[i]),
      `${fee} paid out does not match what the pool owed`
    );
  }

  it("admits an inert hook extension but not a dormant one without the whitelist", async () => {
    // Dormant: no program yet, but an authority that can point the mint at any program with
    // `UpdateTransferHook` once the pool exists. It must not get in through `initialize`.
    const dormant = await createMintWithHookExtension(null);
    let err: any;
    try {
      await initializeWith(dormant, 30);
    } catch (e) {
      err = e;
    }
    assert.isDefined(err, "a dormant hook mint got in without the whitelist");
    assert.include(JSON.stringify(err), "NotSupportMint", `unexpected: ${err}`);

    // token-2022 refuses to initialize the extension with neither field set; an inert one is
    // made by dropping the authority afterwards.
    const inert = await createMintWithHookExtension(null);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createSetAuthorityInstruction(
          inert,
          owner.publicKey,
          AuthorityType.TransferHookProgramId,
          null,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );
    await initializeWith(inert, 37);
  });

  it("rejects a mint that really points at a hook program", async () => {
    const mint = await createMintWithHookExtension(STUB_ID);
    let err: any;
    try {
      await initializeWith(mint, 31);
    } catch (e) {
      err = e;
    }
    assert.isDefined(
      err,
      "a hook mint created a pool without being whitelisted"
    );
    assert.include(JSON.stringify(err), "NotSupportMint", `unexpected: ${err}`);
  });

  it("creates a pool for a whitelisted hook mint and moves its tokens", async () => {
    const mint = await createMintWithHookExtension(STUB_ID);
    const supportMint = await whitelist(mint);
    const list = await createExtraAccountMetaList(
      connection,
      owner,
      mint,
      STUB_ID
    );

    const listAccount = await connection.getAccountInfo(list);
    assert.isNotNull(listAccount, "ExtraAccountMetaList was not created");
    assert.equal(listAccount!.owner.toBase58(), STUB_ID.toBase58());

    // One unordered bag serves both jobs: the whitelist PDA and the mint's list PDA are
    // looked up by key for admission, and the hook program plus the list PDA are what
    // `transfer_checked_with_hook` hands to token-2022 for the actual transfer.
    const { poolState } = await initializeWith(mint, 32, [
      supportMint,
      STUB_ID,
      list,
    ]);

    // The pool exists, which means both vault transfers went through the hook CPI.
    const hookIsToken0 = poolState.token0Mint.toBase58() === mint.toBase58();
    assert.isTrue(
      hookIsToken0 || poolState.token1Mint.toBase58() === mint.toBase58()
    );
    const vault = hookIsToken0 ? poolState.token0Vault : poolState.token1Vault;
    const balance = await connection.getTokenAccountBalance(vault);
    assert.equal(balance.value.amount, "1000");
  });

  it("rejects a list with more extras than a transaction can carry", async () => {
    const mint = await createMintWithHookExtension(STUB_ID);
    const supportMint = await whitelist(mint);
    // MAX_HOOK_EXTRA_ACCOUNTS is 6; seven literal addresses is one too many.
    const list = await createExtraAccountMetaList(
      connection,
      owner,
      mint,
      STUB_ID,
      Array.from({ length: 7 }, () => metaLiteral(Keypair.generate().publicKey))
    );
    let err: any;
    try {
      await initializeWith(mint, 33, [supportMint, STUB_ID, list]);
    } catch (e) {
      err = e;
    }
    assert.isDefined(err, "a list of 7 extras was accepted");
    assert.include(
      JSON.stringify(err),
      "HookExtraAccountsTooMany",
      `unexpected: ${err}`
    );
  });

  it("rejects a list whose address depends on the transfer amount", async () => {
    // The only thing in the Execute instruction data is the amount, and on an AMM at least
    // one leg's amount is computed on chain - so this mint's account list can never be
    // assembled off chain, and the pool must not be allowed to exist.
    const mint = await createMintWithHookExtension(STUB_ID);
    const supportMint = await whitelist(mint);
    const list = await createExtraAccountMetaList(
      connection,
      owner,
      mint,
      STUB_ID,
      [metaSeededByAmount()]
    );
    let err: any;
    try {
      await initializeWith(mint, 34, [supportMint, STUB_ID, list]);
    } catch (e) {
      err = e;
    }
    assert.isDefined(err, "an amount-seeded extra account was accepted");
    assert.include(
      JSON.stringify(err),
      "HookExtraAccountUsesInstructionData",
      `unexpected: ${err}`
    );
  });

  it("rejects a list whose address depends on token account data the transfer changes", async () => {
    // token-2022 resolves the list after moving the balances (and setting the `transferring`
    // flags), while the AMM resolves it before the CPI - so anything past a token account's
    // `mint` and `owner` reads differently on the two sides and the transfer can never match.
    const cases: [string, Buffer, number][] = [
      [
        "a PDA seeded by the source balance",
        metaSeededByAccountData(0, 64, 8),
        39,
      ],
      [
        "an address read past the destination owner",
        metaReadFromAccountData(2, 64),
        40,
      ],
    ];
    for (const [what, meta, configIndex] of cases) {
      const mint = await createMintWithHookExtension(STUB_ID);
      const supportMint = await whitelist(mint);
      const list = await createExtraAccountMetaList(
        connection,
        owner,
        mint,
        STUB_ID,
        [meta]
      );
      let err: any;
      try {
        await initializeWith(mint, configIndex, [supportMint, STUB_ID, list]);
      } catch (e) {
        err = e;
      }
      assert.isDefined(err, `${what} was accepted`);
      assert.include(
        JSON.stringify(err),
        "HookExtraAccountUsesTransferredData",
        `${what}: unexpected: ${err}`
      );
    }
  });

  it("accepts a list that reads the destination owner", async () => {
    // `mint` and `owner` never change in a transfer, so this is resolvable on both sides. On
    // the initial deposit the destination is a vault, whose owner is the pool authority.
    const mint = await createMintWithHookExtension(STUB_ID);
    const supportMint = await whitelist(mint);
    const list = await createExtraAccountMetaList(
      connection,
      owner,
      mint,
      STUB_ID,
      [metaReadFromAccountData(2, 32)]
    );
    const [authority] = await getAuthAddress(program.programId);
    await initializeWith(mint, 41, [supportMint, STUB_ID, list, authority]);
  });

  it("swaps, deposits and withdraws through a hook that declares a real extra account", async () => {
    // An empty list only proves the program and the list PDA get forwarded. One literal extra
    // makes `add_extra_accounts_for_execute_cpi` actually resolve the list and append it, which
    // is what every real hook does.
    const extra = Keypair.generate().publicKey;
    const { pair, hookAccounts } = await createHookPool(35, () => [extra]);
    const hookBag = asMetas(hookAccounts);

    // Each of these moves the hook mint in a different direction, through a different
    // instruction, with the hook accounts as plain trailing accounts. The hook mint is
    // whichever side sorting put it on - it moves either way, so the hook CPI is exercised
    // regardless.
    await swap_base_input(
      program,
      owner,
      ...pair,
      new BN(1_000),
      new BN(0),
      confirmOptions,
      undefined,
      hookBag
    );
    await deposit(
      program,
      owner,
      ...pair,
      new BN(1_000),
      new BN(100_000),
      new BN(100_000),
      confirmOptions,
      hookBag
    );
    await withdraw(
      program,
      owner,
      ...pair,
      new BN(500),
      new BN(0),
      new BN(0),
      confirmOptions,
      hookBag
    );
  });

  it("trades through a hook that declares the pool and its observation as extra accounts", async () => {
    // The pool PDA is fixed by (config, mint0, mint1), so a hook can name it before the pool
    // exists. cp-swap must not hold a borrow on either account across the transfer CPI, or
    // `invoke_signed` fails its borrow check with `AccountBorrowFailed`.
    const { pair, hookAccounts } = await createHookPool(
      36,
      ({ poolAddress, observation }) => [poolAddress, observation]
    );
    const hookBag = asMetas(hookAccounts);

    await swap_base_input(
      program,
      owner,
      ...pair,
      new BN(1_000),
      new BN(0),
      confirmOptions,
      undefined,
      hookBag
    );
    await swap_base_output(
      program,
      owner,
      ...pair,
      new BN(500),
      new BN(10_000),
      confirmOptions,
      undefined,
      hookBag
    );
    await deposit(
      program,
      owner,
      ...pair,
      new BN(1_000),
      new BN(100_000),
      new BN(100_000),
      confirmOptions,
      hookBag
    );
    await withdraw(
      program,
      owner,
      ...pair,
      new BN(500),
      new BN(0),
      new BN(0),
      confirmOptions,
      hookBag
    );
  });

  it("fails a swap that leaves out any of the hook accounts", async () => {
    // The stub checks what it receives like a real hook does, so this is what proves the
    // happy-path tests actually forward the list and its extras rather than just the program.
    const extra = Keypair.generate().publicKey;
    const { pair, list } = await createHookPool(38, () => [extra]);

    const swapWith = (keys: PublicKey[]) =>
      swap_base_input(
        program,
        owner,
        ...pair,
        new BN(1_000),
        new BN(0),
        confirmOptions,
        undefined,
        asMetas(keys)
      );

    // Without the list the interface helpers silently forward nothing but the program, and
    // token-2022 would call the hook with the four transfer accounts only. The program has to
    // refuse that itself rather than trust every hook to notice.
    let err: any;
    try {
      await swapWith([STUB_ID, extra]);
    } catch (e) {
      err = e;
    }
    assert.isDefined(
      err,
      "a swap without the ExtraAccountMetaList went through"
    );
    assert.include(
      JSON.stringify(err),
      "NoExtraAccountsForTransferHook",
      `unexpected: ${err}`
    );

    // Without an extra, resolution fails inside cp-swap before token-2022 is reached.
    err = undefined;
    try {
      await swapWith([STUB_ID, list]);
    } catch (e) {
      err = e;
    }
    assert.isDefined(
      err,
      "a swap without the declared extra account went through"
    );

    // The full set still works, so the failures above are about the missing accounts.
    await swapWith([STUB_ID, list, extra]);
  });

  it("collects protocol and fund fees through a hook", async () => {
    const { pair, poolAddress, hookAccounts } = await createHookPool(
      42,
      ({ poolAddress }) => [poolAddress],
      { rates: COLLECTABLE_FEES }
    );
    const hookBag = asMetas(hookAccounts);
    await tradeBothWays(pair, poolAddress, hookBag);
    const a = await collectAccounts(pair, poolAddress);
    const recipients: [PublicKey, PublicKey] = [a.ownerToken0, a.ownerToken1];
    const accounts = {
      owner: owner.publicKey,
      poolState: a.poolState,
      ammConfig: a.ammConfig,
      token0Vault: a.token0Vault,
      token1Vault: a.token1Vault,
      vault0Mint: a.vault0Mint,
      vault1Mint: a.vault1Mint,
      recipientToken0Account: a.ownerToken0,
      recipientToken1Account: a.ownerToken1,
    };
    const all = new BN("18446744073709551615");

    await expectCollected(poolAddress, "protocolFees", recipients, () =>
      program.methods
        .collectProtocolFee(all, all)
        .accountsPartial(accounts)
        .remainingAccounts(hookBag)
        .rpc(confirmOptions)
    );
    await expectCollected(poolAddress, "fundFees", recipients, () =>
      program.methods
        .collectFundFee(all, all)
        .accountsPartial(accounts)
        .remainingAccounts(hookBag)
        .rpc(confirmOptions)
    );
  });

  it("creates a hook pool with permission and collects its creator fee both ways", async () => {
    // Also the only coverage of `initialize_with_permission` for a hook mint: the payer here
    // is not the launchpad, so the whitelist still applies.
    // The hook names the pool, so this also proves both collects release their borrow on it
    // before the transfer CPI.
    const { pair, poolAddress, hookAccounts } = await createHookPool(
      43,
      ({ poolAddress }) => [poolAddress],
      { rates: COLLECTABLE_FEES, withPermission: true }
    );
    const hookBag = asMetas(hookAccounts);
    const a = await collectAccounts(pair, poolAddress);
    const recipients: [PublicKey, PublicKey] = [a.ownerToken0, a.ownerToken1];
    const accounts = {
      poolState: a.poolState,
      token0Vault: a.token0Vault,
      token1Vault: a.token1Vault,
      vault0Mint: a.vault0Mint,
      vault1Mint: a.vault1Mint,
      token0Program: a.token0Program,
      token1Program: a.token1Program,
    };
    // A non-zero share proves the creator is paid the settled amount, not the gross fee.
    const shareRate = 200_000n;
    await createCreatorFeeShare(
      program,
      owner,
      owner.publicKey,
      a.ammConfig,
      new BN(shareRate.toString()),
      confirmOptions
    );

    await tradeBothWays(pair, poolAddress, hookBag);
    await expectCollected(
      poolAddress,
      "creatorFees",
      recipients,
      () =>
        program.methods
          .collectCreatorFee()
          .accountsPartial({
            ...accounts,
            creator: owner.publicKey,
            ammConfig: a.ammConfig,
          })
          .remainingAccounts(hookBag)
          .rpc(confirmOptions),
      shareRate
    );

    await tradeBothWays(pair, poolAddress, hookBag);
    await expectCollected(
      poolAddress,
      "creatorFees",
      recipients,
      () =>
        program.methods
          .collectCreatorFeePermissionless()
          .accountsPartial({
            ...accounts,
            payer: owner.publicKey,
            creator: owner.publicKey,
            ammConfig: a.ammConfig,
          })
          .remainingAccounts(hookBag)
          .rpc(confirmOptions),
      shareRate
    );
  });

  it("trades a pool where both mints carry a hook", async () => {
    // Two lists and their extras share one unordered bag; each transfer must pick out its
    // own mint's accounts and nothing else.
    const mints = [
      await createMintWithHookExtension(STUB_ID),
      await createMintWithHookExtension(STUB_ID),
    ].sort((x, y) => x.toBuffer().compare(y.toBuffer()));
    const supports = [];
    const bag: PublicKey[] = [STUB_ID];
    for (const mint of mints) {
      supports.push(await whitelist(mint));
      await fundMint(mint, 1_000_000_000);
      const extra = Keypair.generate().publicKey;
      bag.push(
        await createExtraAccountMetaList(connection, owner, mint, STUB_ID, [
          metaLiteral(extra),
        ]),
        extra
      );
    }
    const { configAddress } = await setupInitializeTest(
      program,
      connection,
      owner,
      { config_index: 44, ...LOW_FEES },
      { transferFeeBasisPoints: 0, MaxFee: 0 },
      confirmOptions
    );
    const pair = [
      configAddress,
      mints[0],
      TOKEN_2022_PROGRAM_ID,
      mints[1],
      TOKEN_2022_PROGRAM_ID,
    ] as const;
    const hookBag = asMetas(bag);

    const { poolAddress } = await initialize(
      program,
      owner,
      ...pair,
      confirmOptions,
      { initAmount0: new BN(100_000), initAmount1: new BN(100_000) },
      undefined,
      asMetas([...supports, ...bag])
    );
    const { openTime } = await program.account.poolState.fetch(poolAddress);
    await waitUntilPoolOpen(connection, owner, openTime.toNumber());

    await tradeBothWays(pair, poolAddress, hookBag);
    await deposit(
      program,
      owner,
      ...pair,
      new BN(1_000),
      new BN(100_000),
      new BN(100_000),
      confirmOptions,
      hookBag
    );
    await withdraw(
      program,
      owner,
      ...pair,
      new BN(500),
      new BN(0),
      new BN(0),
      confirmOptions,
      hookBag
    );
  });
});
