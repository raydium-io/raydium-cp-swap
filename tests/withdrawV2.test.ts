import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import { depositV2, setupDepositV2, withdrawV2 } from "./utils";
import { assert } from "chai";
import { getAccount, getAssociatedTokenAddressSync } from "@solana/spl-token";

describe("withdraw v2 test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  it("withdraw v2", async () => {
    const transferFeeConfig = { transferFeeBasisPoints: 0, MaxFee: 5000 };
    const { configAddress, poolAddress, poolState, positionNftMintAddress } =
      await setupDepositV2(
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
        transferFeeConfig,
        2 // collect fee on token1
      );
    const token0AccountAddr = getAssociatedTokenAddressSync(
      poolState.token0Mint,
      owner.publicKey,
      false,
      poolState.token0Program
    );
    const token1AccountAddr = getAssociatedTokenAddressSync(
      poolState.token1Mint,
      owner.publicKey,
      false,
      poolState.token1Program
    );
    const token0AccountBefore = await getAccount(
      anchor.getProvider().connection,
      token0AccountAddr,
      "processed",
      poolState.token0Program
    );

    const token1AccountBefore = await getAccount(
      anchor.getProvider().connection,
      token1AccountAddr,
      "processed",
      poolState.token1Program
    );
    const lp_amount = new BN(10000000000);
    await depositV2(
      program,
      owner,
      poolAddress,
      positionNftMintAddress,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      lp_amount,
      new BN(100000000000),
      new BN(100000000000),
      confirmOptions
    );
    const token0AccountAfterDeposit = await getAccount(
      anchor.getProvider().connection,
      token0AccountAddr,
      "processed",
      poolState.token0Program
    );

    const token1AccountAfterDeposit = await getAccount(
      anchor.getProvider().connection,
      token1AccountAddr,
      "processed",
      poolState.token1Program
    );

    const token0AmountDeposit =
      token0AccountBefore.amount - token0AccountAfterDeposit.amount;

    const token1AmountDeposit =
      token1AccountBefore.amount - token1AccountAfterDeposit.amount;

    await withdrawV2(
      program,
      owner,
      poolAddress,
      positionNftMintAddress,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      lp_amount,
      new BN(0),
      new BN(0),
      confirmOptions
    );

    const token0AccountAfterWithdraw = await getAccount(
      anchor.getProvider().connection,
      token0AccountAddr,
      "processed",
      poolState.token0Program
    );

    const token1AccountAfterWithdraw = await getAccount(
      anchor.getProvider().connection,
      token1AccountAddr,
      "processed",
      poolState.token1Program
    );

    assert.equal(
      token0AmountDeposit - 1n,
      token0AccountAfterWithdraw.amount - token0AccountAfterDeposit.amount
    );

    assert.equal(
      token1AmountDeposit - 1n,
      token1AccountAfterWithdraw.amount - token1AccountAfterDeposit.amount
    );
  });
});

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
