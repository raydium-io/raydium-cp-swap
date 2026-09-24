import {
  Connection,
  PublicKey,
  Signer,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

const UPGRADEABLE_LOADER = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);
const MAINNET_RPC = "https://api.mainnet-beta.solana.com";

async function rpcCall(endpoint: string, method: string, params: any[]) {
  const response = await (globalThis as any).fetch(endpoint, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const json = await response.json();
  if (json.error) {
    throw new Error(`${method}: ${JSON.stringify(json.error)}`);
  }
  return json.result;
}

// The surfnet started by `anchor test` ships the bundled legacy token program
// build, while mainnet runs the p-token redeployment (which, unlike the
// bundled build, supports WithdrawExcessLamports). Replace the local token
// program with the live mainnet deployment via the surfnet_setAccount
// cheatcode so legacy-token behavior matches mainnet.
// Returns false (so the caller skips the legacy-token cases) when the local
// validator is not a surfnet, or when the mainnet deployment cannot be
// fetched — a network blip must not fail the whole suite.
export async function ensureMainnetTokenProgram(
  connection: Connection
): Promise<boolean> {
  try {
    const version = await rpcCall(connection.rpcEndpoint, "getVersion", []);
    if (version["surfnet-version"] === undefined) {
      return false;
    }

    const local = await connection.getAccountInfo(TOKEN_PROGRAM_ID);
    if (local.owner.equals(UPGRADEABLE_LOADER)) {
      // mainnet deployment already installed
      return true;
    }

    const [programdataAddress] = PublicKey.findProgramAddressSync(
      [TOKEN_PROGRAM_ID.toBuffer()],
      UPGRADEABLE_LOADER
    );
    const mainnet = new Connection(MAINNET_RPC);
    const [programAccount, programdataAccount] =
      await mainnet.getMultipleAccountsInfo([
        TOKEN_PROGRAM_ID,
        programdataAddress,
      ]);

    // install programdata first so the program account never points at nothing
    for (const [address, account] of [
      [programdataAddress, programdataAccount],
      [TOKEN_PROGRAM_ID, programAccount],
    ] as const) {
      await rpcCall(connection.rpcEndpoint, "surfnet_setAccount", [
        address.toBase58(),
        {
          lamports: account.lamports,
          data: account.data.toString("hex"),
          owner: account.owner.toBase58(),
          executable: account.executable,
          rent_epoch: 0,
        },
      ]);
    }
    return true;
  } catch (err) {
    console.warn(
      "ensureMainnetTokenProgram: could not install mainnet token program, " +
        "skipping legacy-token cases:",
      err instanceof Error ? err.message : err
    );
    return false;
  }
}

const PROGRAM_TAG = 2;
const PROGRAMDATA_TAG = 3;

/// Install `programs/transfer-hook-stub` into the surfnet as an **upgradeable** program whose
/// upgrade authority is `upgradeAuthority`.
///
/// It is written straight into the ledger rather than added to `[programs.Localnet]`: the
/// launchpad's own deploy is what makes `LanMV9s...` executable here, and adding a second
/// program to that list breaks it. Writing the accounts also lets the test choose the upgrade
/// authority, which is the whole point — the launchpad only accepts a hook program whose
/// upgrade authority re-derives to the platform config.
export async function installStubHookProgram(
  connection: Connection,
  programId: PublicKey,
  soPath: string,
  upgradeAuthority: PublicKey
): Promise<void> {
  const fs = await import("fs");
  const elf = fs.readFileSync(soPath);

  const [programdataAddress] = PublicKey.findProgramAddressSync(
    [programId.toBuffer()],
    UPGRADEABLE_LOADER
  );

  // UpgradeableLoaderState::ProgramData { slot, Some(authority) } then the ELF
  const programdata = Buffer.concat([
    u32le(PROGRAMDATA_TAG),
    Buffer.alloc(8), // slot 0
    Buffer.from([1]),
    upgradeAuthority.toBuffer(),
    elf,
  ]);
  // UpgradeableLoaderState::Program { programdata_address }
  const program = Buffer.concat([
    u32le(PROGRAM_TAG),
    programdataAddress.toBuffer(),
  ]);

  // programdata first, so the program account never points at nothing
  for (const [address, data, executable] of [
    [programdataAddress, programdata, false],
    [programId, program, true],
  ] as const) {
    await rpcCall(connection.rpcEndpoint, "surfnet_setAccount", [
      address.toBase58(),
      {
        lamports: 10 * 1e9,
        data: data.toString("hex"),
        owner: UPGRADEABLE_LOADER.toBase58(),
        executable,
        rent_epoch: 0,
      },
    ]);
  }
}

function u32le(value: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(value);
  return buf;
}

const EXTRA_ACCOUNT_METAS_SEED = Buffer.from("extra-account-metas");
// sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[..8]
const INIT_LIST_DISCRIMINATOR = Buffer.from([
  43, 34, 13, 49, 167, 88, 235, 235,
]);

/// One `ExtraAccountMeta` as the on chain TLV stores it: 35 bytes.
export function extraAccountMeta(
  discriminator: number,
  addressConfig: Buffer,
  isSigner = false,
  isWritable = false
): Buffer {
  const config = Buffer.alloc(32);
  addressConfig.copy(config);
  return Buffer.concat([
    Buffer.from([discriminator]),
    config,
    Buffer.from([isSigner ? 1 : 0, isWritable ? 1 : 0]),
  ]);
}

/// A meta whose address is a PDA seeded from the Execute instruction data - i.e. from the
/// transfer amount. `Seed::InstructionData { index, length }` packs as `[2, index, length]`.
export function metaSeededByAmount(): Buffer {
  return extraAccountMeta(1, Buffer.from([2, 0, 8]));
}

/// A meta whose address is a PDA seeded from an account's data.
/// `Seed::AccountData { account_index, data_index, length }` packs as `[4, index, offset, length]`.
export function metaSeededByAccountData(
  accountIndex: number,
  dataIndex: number,
  length: number
): Buffer {
  return extraAccountMeta(1, Buffer.from([4, accountIndex, dataIndex, length]));
}

/// A meta whose address is 32 bytes read out of an account's data.
/// `PubkeyData::AccountData { account_index, data_index }` packs as `[2, index, offset]`.
export function metaReadFromAccountData(
  accountIndex: number,
  dataIndex: number
): Buffer {
  return extraAccountMeta(2, Buffer.from([2, accountIndex, dataIndex]));
}

/// A meta that is just a fixed address.
export function metaLiteral(address: PublicKey): Buffer {
  return extraAccountMeta(0, address.toBuffer());
}

export function extraAccountMetaListAddress(
  mint: PublicKey,
  hookProgram: PublicKey
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [EXTRA_ACCOUNT_METAS_SEED, mint.toBuffer()],
    hookProgram
  )[0];
}

/// Create a mint's `ExtraAccountMetaList` through the stub hook, the way a real hook program
/// would: the caller funds the PDA first, the program only allocates and writes.
export async function createExtraAccountMetaList(
  connection: Connection,
  payer: Signer,
  mint: PublicKey,
  hookProgram: PublicKey,
  metas: Buffer[] = []
): Promise<PublicKey> {
  const list = extraAccountMetaListAddress(mint, hookProgram);
  const count = Buffer.alloc(4);
  count.writeUInt32LE(metas.length);
  const data = Buffer.concat([INIT_LIST_DISCRIMINATOR, count, ...metas]);

  const tx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: list,
      lamports: await connection.getMinimumBalanceForRentExemption(
        64 + metas.length * 35
      ),
    }),
    new TransactionInstruction({
      programId: hookProgram,
      keys: [
        { pubkey: list, isSigner: false, isWritable: true },
        { pubkey: mint, isSigner: false, isWritable: false },
        { pubkey: payer.publicKey, isSigner: true, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data,
    })
  );
  await sendAndConfirmTransaction(connection, tx, [payer]);
  return list;
}
