import * as anchor from "@anchor-lang/core";
import {
  Connection,
  Signer,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  ConfirmOptions,
  sendAndConfirmTransaction,
} from "@solana/web3.js";

// web3.js reuses one blockhash for 30s (`BLOCKHASH_CACHE_TIMEOUT_MS`) across every
// `Connection.sendTransaction(tx, signers)` call - including the ones inside `@solana/spl-token`,
// which this suite cannot reach. Surfpool only produces a block when a transaction lands, so a
// burst of transactions rolls its blockhash queue past whatever the cache is still handing out,
// and the next call dies with `Blockhash not found`. Turning the cache off costs one extra RPC
// per transaction, which is free against a local validator.
const __providerEnv = anchor.AnchorProvider.env.bind(anchor.AnchorProvider);
anchor.AnchorProvider.env = () => {
  const provider = __providerEnv();
  (provider.connection as any)._disableBlockhashCaching = true;
  return provider;
};

export async function accountExist(
  connection: anchor.web3.Connection,
  account: anchor.web3.PublicKey
) {
  const info = await connection.getAccountInfo(account);
  if (info == null || info.data.length == 0) {
    return false;
  }
  return true;
}

export async function sendTransaction(
  connection: Connection,
  ixs: TransactionInstruction[],
  signers: Array<Signer>,
  options?: ConfirmOptions
): Promise<TransactionSignature> {
  if (options == undefined) {
    options = {
      preflightCommitment: "confirmed",
      commitment: "confirmed",
    };
  }

  const preflightCommitment = options.preflightCommitment || options.commitment;
  const sendOpt = {
    skipPreflight: options.skipPreflight,
    preflightCommitment,
  };

  // Sign and send raw: `connection.sendTransaction(tx, signers)` swaps in a blockhash from
  // web3.js's 30s cache, which surfpool's fast-rolling queue may no longer hold. The
  // blockhash is fetched at the preflight commitment, and only `Blockhash not found`
  // (a hash expiring between fetch and send) is retried.
  let signature: TransactionSignature;
  for (let attempt = 0; ; attempt++) {
    const tx = new Transaction();
    for (var i = 0; i < ixs.length; i++) {
      tx.add(ixs[i]);
    }
    const { blockhash, lastValidBlockHeight } =
      await connection.getLatestBlockhash(preflightCommitment);
    tx.recentBlockhash = blockhash;
    tx.lastValidBlockHeight = lastValidBlockHeight;
    tx.feePayer = signers[0].publicKey;
    tx.sign(...signers);

    try {
      signature = await connection.sendRawTransaction(tx.serialize(), sendOpt);
      break;
    } catch (e) {
      if (attempt >= 5 || !String(e).includes("Blockhash not found")) {
        throw e;
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }

  const status = (
    await connection.confirmTransaction(signature, options.commitment)
  ).value;

  if (status.err) {
    throw new Error(
      `Raw transaction ${signature} failed (${JSON.stringify(status)})`
    );
  }
  return signature;
}

export async function getBlockTimestamp(
  connection: Connection
): Promise<number> {
  let slot = await connection.getSlot();
  return await connection.getBlockTime(slot);
}

/// `initialize` and `initialize_with_permission` clamp a pool's `open_time` to
/// `block_timestamp + 1`, so the first swap on a fresh pool can be rejected with
/// `NotApproved`. Retry instead of sleeping: the surfnet clock advances with the
/// transactions it processes, so waiting without sending anything never clears it.
/// Only that one error is retried, so a real failure still surfaces immediately.
export async function retryUntilPoolOpen<T>(send: () => Promise<T>): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await send();
    } catch (e) {
      if (attempt >= 20 || !isPoolNotOpenYet(e)) {
        throw e;
      }
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }
}

/// `NotApproved` is the program's first error variant, so it is custom error 6000.
/// A preflight failure arrives as a parsed AnchorError naming it, while a transaction
/// sent with `skipPreflight` arrives as a raw rpc object that stringifies to
/// "[object Object]" - so both shapes have to be matched.
const NOT_APPROVED_ERROR_CODE = 6000;

function isPoolNotOpenYet(e: unknown): boolean {
  let text = String(e);
  try {
    text += JSON.stringify(e);
  } catch {
    // a circular error object, the string form is all we get
  }
  return (
    text.includes("NotApproved") ||
    text.includes(`"Custom":${NOT_APPROVED_ERROR_CODE}`) ||
    text.includes(`"Custom": ${NOT_APPROVED_ERROR_CODE}`)
  );
}

/// Block until the chain clock has passed `openTime`.
///
/// `initialize` forces `open_time = block_timestamp + 1`, and both swap instructions reject with
/// `NotApproved` while `block_timestamp < open_time` - so a pool is untradeable for one second
/// after it is created. Surfpool runs with `--block-production-mode transaction`: no transaction,
/// no block, no clock movement, so waiting passively never works. Each iteration has to land a
/// transaction to push a block out.
export async function waitUntilPoolOpen(
  connection: Connection,
  payer: Signer,
  openTime: number
): Promise<void> {
  for (let i = 0; i < 20; i++) {
    const chainTime = await connection.getBlockTime(await connection.getSlot());
    if (chainTime !== null && chainTime >= openTime) return;
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: payer.publicKey,
          toPubkey: payer.publicKey,
          lamports: 1,
        })
      ),
      [payer]
    );
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`pool never opened (open_time ${openTime})`);
}
