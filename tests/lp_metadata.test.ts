import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  setupInitializeTest,
  initialize,
  getAuthAddress,
  getPoolLpMintAddress,
} from "./utils";
import { assert } from "chai";

const METADATA_PROGRAM_ID = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);
const CREATE_POOL_FEE_RECEIVER = new PublicKey(
  "DNXgeM9EiiaAbaWvwjHj9fQQLAX5ZsfHyvmYUNRAdNC8"
);
const LP_METADATA_STATE_SEED = Buffer.from("lp_metadata_state");
const METADATA_SEED = Buffer.from("metadata");
const LP_METADATA_FEE = 100_000_000; // 0.1 SOL

async function assertRpcRejected(promise: Promise<unknown>): Promise<void> {
  let rejected = false;
  try {
    await promise;
  } catch {
    rejected = true;
  }
  assert.isTrue(rejected, "expected rpc call to be rejected");
}

describe("lp metadata test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;
  const connection = anchor.getProvider().connection;
  const confirmOptions = {
    skipPreflight: true,
  };

  it("create_lp_metadata + update_lp_metadata end-to-end", async () => {
    // 1. Create a pool using the cloned index-0 amm config.
    const { configAddress, token0, token0Program, token1, token1Program } =
      await setupInitializeTest(
        program,
        connection,
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
    const { poolAddress } = await initialize(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions,
      { initAmount0: new BN(10000000000), initAmount1: new BN(10000000000) }
    );

    const [auth] = await getAuthAddress(program.programId);
    const [lpMint] = await getPoolLpMintAddress(poolAddress, program.programId);
    const [lpMetadataState] = await PublicKey.findProgramAddress(
      [LP_METADATA_STATE_SEED, poolAddress.toBuffer()],
      program.programId
    );
    const [metadata] = await PublicKey.findProgramAddress(
      [METADATA_SEED, METADATA_PROGRAM_ID.toBuffer(), lpMint.toBuffer()],
      METADATA_PROGRAM_ID
    );

    const feeBefore = await connection.getBalance(CREATE_POOL_FEE_RECEIVER);

    // 2. Anyone (permissionless) creates the LP metadata.
    await program.methods
      .createLpMetadata("RAID MEME LP", "MEME", "https://example.com/meme.json")
      .accounts({
        payer: owner.publicKey,
        poolState: poolAddress,
        lpMint,
        lpMetadataState,
        authority: auth,
        metadata,
        createPoolFeeReveiver: CREATE_POOL_FEE_RECEIVER,
        tokenProgram: TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .rpc(confirmOptions);

    // 3. The metaplex metadata account now exists and is owned by the metadata program.
    const metadataInfo = await connection.getAccountInfo(metadata);
    assert.isNotNull(metadataInfo, "metadata account should exist");
    assert.equal(
      metadataInfo!.owner.toString(),
      METADATA_PROGRAM_ID.toString()
    );

    // 4. The metadata state records owner as the sole creator.
    const state = await program.account.lpMetadataState.fetch(lpMetadataState);
    assert.equal(state.creator.toString(), owner.publicKey.toString());

    // 5. Exactly 0.1 SOL was charged to the Raydium fee receiver.
    const feeAfter = await connection.getBalance(CREATE_POOL_FEE_RECEIVER);
    assert.equal(feeAfter - feeBefore, LP_METADATA_FEE);

    // Fund a second, distinct wallet.
    const second = Keypair.generate();
    const fundTx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: owner.publicKey,
        toPubkey: second.publicKey,
        lamports: anchor.web3.LAMPORTS_PER_SOL,
      })
    );
    await sendAndConfirmTransaction(connection, fundTx, [owner]);

    // 6. First-come-first-serve: a second creator cannot create again.
    await assertRpcRejected(
      program.methods
        .createLpMetadata("SQUATTER", "SQTR", "https://example.com/squat.json")
        .accounts({
          payer: second.publicKey,
          poolState: poolAddress,
          lpMint,
          lpMetadataState,
          authority: auth,
          metadata,
          createPoolFeeReveiver: CREATE_POOL_FEE_RECEIVER,
          tokenProgram: TOKEN_PROGRAM_ID,
          metadataProgram: METADATA_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: SYSVAR_RENT_PUBKEY,
        })
        .signers([second])
        .rpc(confirmOptions)
    );

    // 7. Only the creator can update: a non-creator is rejected.
    await assertRpcRejected(
      program.methods
        .updateLpMetadata("HACK", "HACK", "https://example.com/hack.json")
        .accounts({
          updater: second.publicKey,
          poolState: poolAddress,
          lpMint,
          lpMetadataState,
          ammConfig: configAddress,
          authority: auth,
          metadata,
          metadataProgram: METADATA_PROGRAM_ID,
        })
        .signers([second])
        .rpc(confirmOptions)
    );

    // 8. The creator can update.
    await program.methods
      .updateLpMetadata(
        "RENAMED MEME LP",
        "MEME2",
        "https://example.com/meme2.json"
      )
      .accounts({
        updater: owner.publicKey,
        poolState: poolAddress,
        lpMint,
        lpMetadataState,
        ammConfig: configAddress,
        authority: auth,
        metadata,
        metadataProgram: METADATA_PROGRAM_ID,
      })
      .rpc(confirmOptions);
  });

  it("reclaim_lp_metadata: pool creator overrides a front-running griefer", async () => {
    // 1. Create a pool; `owner` is the pool creator.
    const { configAddress, token0, token0Program, token1, token1Program } =
      await setupInitializeTest(
        program,
        connection,
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
    const { poolAddress } = await initialize(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions,
      { initAmount0: new BN(10000000000), initAmount1: new BN(10000000000) }
    );

    const [auth] = await getAuthAddress(program.programId);
    const [lpMint] = await getPoolLpMintAddress(poolAddress, program.programId);
    const [lpMetadataState] = await PublicKey.findProgramAddress(
      [LP_METADATA_STATE_SEED, poolAddress.toBuffer()],
      program.programId
    );
    const [metadata] = await PublicKey.findProgramAddress(
      [METADATA_SEED, METADATA_PROGRAM_ID.toBuffer(), lpMint.toBuffer()],
      METADATA_PROGRAM_ID
    );

    // Fund a second wallet that front-runs the metadata claim.
    const second = Keypair.generate();
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: owner.publicKey,
          toPubkey: second.publicKey,
          lamports: anchor.web3.LAMPORTS_PER_SOL,
        })
      ),
      [owner]
    );

    // 2. The griefer claims the metadata first, becoming its creator.
    await program.methods
      .createLpMetadata("SQUATTER", "SQTR", "https://example.com/squat.json")
      .accounts({
        payer: second.publicKey,
        poolState: poolAddress,
        lpMint,
        lpMetadataState,
        authority: auth,
        metadata,
        createPoolFeeReveiver: CREATE_POOL_FEE_RECEIVER,
        tokenProgram: TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([second])
      .rpc(confirmOptions);

    let state = await program.account.lpMetadataState.fetch(lpMetadataState);
    assert.equal(state.creator.toString(), second.publicKey.toString());

    // 3. A random third wallet cannot reclaim.
    const third = Keypair.generate();
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: owner.publicKey,
          toPubkey: third.publicKey,
          lamports: anchor.web3.LAMPORTS_PER_SOL,
        })
      ),
      [owner]
    );
    await assertRpcRejected(
      program.methods
        .reclaimLpMetadata()
        .accounts({
          reclaimer: third.publicKey,
          poolState: poolAddress,
          ammConfig: configAddress,
          lpMetadataState,
        })
        .signers([third])
        .rpc(confirmOptions)
    );

    // 3b. The pool creator can directly override the metadata without reclaiming.
    await program.methods
      .updateLpMetadata("OVERRIDE", "OVR", "https://example.com/override.json")
      .accounts({
        updater: owner.publicKey,
        poolState: poolAddress,
        lpMint,
        lpMetadataState,
        ammConfig: configAddress,
        authority: auth,
        metadata,
        metadataProgram: METADATA_PROGRAM_ID,
      })
      .rpc(confirmOptions);

    // 4. The pool creator reclaims the update authority from the griefer.
    await program.methods
      .reclaimLpMetadata()
      .accounts({
        reclaimer: owner.publicKey,
        poolState: poolAddress,
        ammConfig: configAddress,
        lpMetadataState,
      })
      .rpc(confirmOptions);

    state = await program.account.lpMetadataState.fetch(lpMetadataState);
    assert.equal(state.creator.toString(), owner.publicKey.toString());

    // 5. The pool creator can now update the metadata as the new creator.
    await program.methods
      .updateLpMetadata(
        "RECLAIMED",
        "RLM",
        "https://example.com/reclaimed.json"
      )
      .accounts({
        updater: owner.publicKey,
        poolState: poolAddress,
        lpMint,
        lpMetadataState,
        ammConfig: configAddress,
        authority: auth,
        metadata,
        metadataProgram: METADATA_PROGRAM_ID,
      })
      .rpc(confirmOptions);
  });
});
