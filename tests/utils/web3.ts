import * as anchor from "@anchor-lang/core";
import {
  Connection,
  Signer,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  ConfirmOptions,
} from "@solana/web3.js";

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

  // The blockhash has to be fetched at the commitment preflight simulates against.
  // Fetching it at the connection's default instead can hand back a blockhash newer
  // than the simulation bank, which fails as `Blockhash not found` - regularly, once
  // the local validator is under load. The bounded retry covers the remaining races
  // (a blockhash that expires between fetch and send); every other error is rethrown
  // immediately so real failures still surface.
  let signature: TransactionSignature;
  for (let attempt = 0; ; attempt++) {
    const tx = new Transaction();
    for (var i = 0; i < ixs.length; i++) {
      tx.add(ixs[i]);
    }
    tx.recentBlockhash = (
      await connection.getLatestBlockhash(preflightCommitment)
    ).blockhash;

    try {
      signature = await connection.sendTransaction(tx, signers, sendOpt);
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
