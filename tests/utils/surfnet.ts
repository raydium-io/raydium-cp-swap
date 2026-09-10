import { Connection, PublicKey } from "@solana/web3.js";
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
