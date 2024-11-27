import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import { setupSwapTest, swap_base_input, swap_base_output } from "./utils";
import { assert } from "chai";
import { getAccount, getAssociatedTokenAddressSync } from "@solana/spl-token";
import { Connection } from "@solana/web3.js";

export enum PoolStatusBitIndex {
  Deposit = 0,
  Withdraw = 1,
  Swap = 2,
}

export function checkPoolStatusBit(
  status: number,
  bit: PoolStatusBitIndex
): boolean {
  const mask = 1 << bit;
  return (status & mask) === 0; // Returns true if enabled (bit is 0)
}

export async function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitForNextBlock(connection: Connection) {
  const slot = await connection.getSlot();
  console.log("Current slot:", slot);

  // Wait for next block
  while ((await connection.getSlot()) <= slot) {
    await sleep(100);
  }
  console.log("New slot:", await connection.getSlot());
}

describe("swap test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  it("swap base input without transfer fee", async () => {
    const { configAddress, poolAddress, poolState } = await setupSwapTest(
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
      { transferFeeBasisPoints: 0, MaxFee: 0 }
    );
    await waitForNextBlock(program.provider.connection);

    const inputToken = poolState.token0Mint;
    const inputTokenProgram = poolState.token0Program;
    const inputTokenAccountAddr = getAssociatedTokenAddressSync(
      inputToken,
      owner.publicKey,
      false,
      inputTokenProgram
    );
    const inputTokenAccountBefore = await getAccount(
      anchor.getProvider().connection,
      inputTokenAccountAddr,
      "processed",
      inputTokenProgram
    );
    let amount_in = new BN(100000000);
    await swap_base_input(
      program,
      owner,
      configAddress,
      inputToken,
      inputTokenProgram,
      poolState.token1Mint,
      poolState.token1Program,
      amount_in,
      new BN(0),
      confirmOptions
    );
    const inputTokenAccountAfter = await getAccount(
      anchor.getProvider().connection,
      inputTokenAccountAddr,
      "processed",
      inputTokenProgram
    );
    assert.equal(
      inputTokenAccountBefore.amount - inputTokenAccountAfter.amount,
      BigInt(amount_in.toString())
    );
  });

  it("swap base output without transfer fee", async () => {
    const { configAddress, poolAddress, poolState } = await setupSwapTest(
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
      { transferFeeBasisPoints: 0, MaxFee: 0 }
    );
    await waitForNextBlock(program.provider.connection);
    const inputToken = poolState.token0Mint;
    const inputTokenProgram = poolState.token0Program;
    const inputTokenAccountAddr = getAssociatedTokenAddressSync(
      inputToken,
      owner.publicKey,
      false,
      inputTokenProgram
    );
    const outputToken = poolState.token1Mint;
    const outputTokenProgram = poolState.token1Program;
    const outputTokenAccountAddr = getAssociatedTokenAddressSync(
      outputToken,
      owner.publicKey,
      false,
      outputTokenProgram
    );
    const outputTokenAccountBefore = await getAccount(
      anchor.getProvider().connection,
      outputTokenAccountAddr,
      "processed",
      outputTokenProgram
    );
    let amount_out = new BN(100000000);
    await swap_base_output(
      program,
      owner,
      configAddress,
      inputToken,
      inputTokenProgram,
      poolState.token1Mint,
      poolState.token1Program,
      amount_out,
      new BN(10000000000000),
      confirmOptions
    );
    const outputTokenAccountAfter = await getAccount(
      anchor.getProvider().connection,
      outputTokenAccountAddr,
      "processed",
      outputTokenProgram
    );
    assert.equal(
      outputTokenAccountAfter.amount - outputTokenAccountBefore.amount,
      BigInt(amount_out.toString())
    );
  });

  it("swap base output with transfer fee", async () => {
    const transferFeeConfig = { transferFeeBasisPoints: 5, MaxFee: 5000 }; // %5
    const { configAddress, poolAddress, poolState } = await setupSwapTest(
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
      transferFeeConfig
    );
    await waitForNextBlock(program.provider.connection);

    const inputToken = poolState.token0Mint;
    const inputTokenProgram = poolState.token0Program;
    const inputTokenAccountAddr = getAssociatedTokenAddressSync(
      inputToken,
      owner.publicKey,
      false,
      inputTokenProgram
    );
    const outputToken = poolState.token1Mint;
    const outputTokenProgram = poolState.token1Program;
    const outputTokenAccountAddr = getAssociatedTokenAddressSync(
      outputToken,
      owner.publicKey,
      false,
      outputTokenProgram
    );
    const outputTokenAccountBefore = await getAccount(
      anchor.getProvider().connection,
      outputTokenAccountAddr,
      "processed",
      outputTokenProgram
    );
    let amount_out = new BN(100000000);
    await swap_base_output(
      program,
      owner,
      configAddress,
      inputToken,
      inputTokenProgram,
      poolState.token1Mint,
      poolState.token1Program,
      amount_out,
      new BN(10000000000000),
      confirmOptions
    );
    const outputTokenAccountAfter = await getAccount(
      anchor.getProvider().connection,
      outputTokenAccountAddr,
      "processed",
      outputTokenProgram
    );
    assert.equal(
      outputTokenAccountAfter.amount - outputTokenAccountBefore.amount,
      BigInt(amount_out.toString())
    );
  });
});
