import * as anchor from "@coral-xyz/anchor";
import { web3 } from "@coral-xyz/anchor";
import {
  Connection,
  PublicKey,
  Keypair,
  Signer,
  TransactionInstruction,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  createMint,
  TOKEN_PROGRAM_ID,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_2022_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  ExtensionType,
  getMintLen,
  createInitializeTransferFeeConfigInstruction,
  createInitializeMintInstruction,
  getAccount,
} from "@solana/spl-token";
import { sendTransaction } from "./index";

// create a token mint and a token2022 mint with transferFeeConfig
export async function createTokenMintAndAssociatedTokenAccount(
  connection: Connection,
  payer: Signer,
  mintAuthority: Signer,
  transferFeeConfig: { transferFeeBasisPoints: number; MaxFee: number }
) {
  let ixs: TransactionInstruction[] = [];
  ixs.push(
    web3.SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: mintAuthority.publicKey,
      lamports: web3.LAMPORTS_PER_SOL,
    })
  );
  await sendTransaction(connection, ixs, [payer]);

  interface Token {
    address: PublicKey;
    program: PublicKey;
  }

  let tokenArray: Token[] = [];
  let token0 = await createMint(
    connection,
    mintAuthority,
    mintAuthority.publicKey,
    null,
    9
  );
  tokenArray.push({ address: token0, program: TOKEN_PROGRAM_ID });

  let token1 = await createMintWithTransferFee(
    connection,
    payer,
    mintAuthority,
    Keypair.generate(),
    transferFeeConfig
  );

  tokenArray.push({ address: token1, program: TOKEN_2022_PROGRAM_ID });

  tokenArray.sort(function (x, y) {
    if (x.address < y.address) {
      return -1;
    }
    if (x.address > y.address) {
      return 1;
    }
    return 0;
  });

  token0 = tokenArray[0].address;
  token1 = tokenArray[1].address;
  //   console.log("Token 0", token0.toString());
  //   console.log("Token 1", token1.toString());
  const token0Program = tokenArray[0].program;
  const token1Program = tokenArray[1].program;

  const ownerToken0Account = await getOrCreateAssociatedTokenAccount(
    connection,
    payer,
    token0,
    payer.publicKey,
    false,
    "processed",
    { skipPreflight: true },
    token0Program
  );

  await mintTo(
    connection,
    payer,
    token0,
    ownerToken0Account.address,
    mintAuthority,
    100_000_000_000_000,
    [],
    { skipPreflight: true },
    token0Program
  );

  // console.log(
  //   "ownerToken0Account key: ",
  //   ownerToken0Account.address.toString()
  // );

  const ownerToken1Account = await getOrCreateAssociatedTokenAccount(
    connection,
    payer,
    token1,
    payer.publicKey,
    false,
    "processed",
    { skipPreflight: true },
    token1Program
  );
  // console.log(
  //   "ownerToken1Account key: ",
  //   ownerToken1Account.address.toString()
  // );
  await mintTo(
    connection,
    payer,
    token1,
    ownerToken1Account.address,
    mintAuthority,
    100_000_000_000_000,
    [],
    { skipPreflight: true },
    token1Program
  );

  return [
    { token0, token0Program },
    { token1, token1Program },
  ];
}

async function createMintWithTransferFee(
  connection: Connection,
  payer: Signer,
  mintAuthority: Signer,
  mintKeypair = Keypair.generate(),
  transferFeeConfig: { transferFeeBasisPoints: number; MaxFee: number }
) {
  const transferFeeConfigAuthority = Keypair.generate();
  const withdrawWithheldAuthority = Keypair.generate();

  const extensions = [ExtensionType.TransferFeeConfig];

  const mintLen = getMintLen(extensions);
  const decimals = 9;

  const mintLamports = await connection.getMinimumBalanceForRentExemption(
    mintLen
  );
  const mintTransaction = new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: mintKeypair.publicKey,
      space: mintLen,
      lamports: mintLamports,
      programId: TOKEN_2022_PROGRAM_ID,
    }),
    createInitializeTransferFeeConfigInstruction(
      mintKeypair.publicKey,
      transferFeeConfigAuthority.publicKey,
      withdrawWithheldAuthority.publicKey,
      transferFeeConfig.transferFeeBasisPoints,
      BigInt(transferFeeConfig.MaxFee),
      TOKEN_2022_PROGRAM_ID
    ),
    createInitializeMintInstruction(
      mintKeypair.publicKey,
      decimals,
      mintAuthority.publicKey,
      null,
      TOKEN_2022_PROGRAM_ID
    )
  );
  await sendAndConfirmTransaction(
    connection,
    mintTransaction,
    [payer, mintKeypair],
    undefined
  );

  return mintKeypair.publicKey;
}

export async function getUserAndPoolVaultAmount(
  owner: PublicKey,
  token0Mint: PublicKey,
  token0Program: PublicKey,
  token1Mint: PublicKey,
  token1Program: PublicKey,
  poolToken0Vault: PublicKey,
  poolToken1Vault: PublicKey
) {
  const onwerToken0AccountAddr = getAssociatedTokenAddressSync(
    token0Mint,
    owner,
    false,
    token0Program
  );

  const onwerToken1AccountAddr = getAssociatedTokenAddressSync(
    token1Mint,
    owner,
    false,
    token1Program
  );

  const onwerToken0Account = await getAccount(
    anchor.getProvider().connection,
    onwerToken0AccountAddr,
    "processed",
    token0Program
  );

  const onwerToken1Account = await getAccount(
    anchor.getProvider().connection,
    onwerToken1AccountAddr,
    "processed",
    token1Program
  );

  const poolVault0TokenAccount = await getAccount(
    anchor.getProvider().connection,
    poolToken0Vault,
    "processed",
    token0Program
  );

  const poolVault1TokenAccount = await getAccount(
    anchor.getProvider().connection,
    poolToken1Vault,
    "processed",
    token1Program
  );
  return {
    onwerToken0Account,
    onwerToken1Account,
    poolVault0TokenAccount,
    poolVault1TokenAccount,
  };
}

export function isEqual(amount1: bigint, amount2: bigint) {
  if (
    BigInt(amount1) === BigInt(amount2) ||
    BigInt(amount1) - BigInt(amount2) === BigInt(1) ||
    BigInt(amount1) - BigInt(amount2) === BigInt(-1)
  ) {
    return true;
  }
  return false;
}
