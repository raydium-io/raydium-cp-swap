import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import {
  collectPositionFee,
  depositV2,
  setupDepositV2,
  swap_base_output,
} from "./utils";
import { assert } from "chai";
import { getAccount, getAssociatedTokenAddressSync } from "@solana/spl-token";

describe("collect position fee test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  it("collect position fee", async () => {
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
        0
      );
    const poolBeforeDeposit = await program.account.poolState.fetch(
      poolAddress
    );
    const lpAmount = poolBeforeDeposit.lpSupply.divn(2);
    const { position } = await depositV2(
      program,
      owner,
      poolAddress,
      positionNftMintAddress,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      lpAmount,
      new BN(100000000000),
      new BN(100000000000),
      confirmOptions
    );
    const poolAfterDeposit = await program.account.poolState.fetch(poolAddress);
    assert.equal(
      poolAfterDeposit.lpSupply.sub(poolBeforeDeposit.lpSupply).toString(),
      lpAmount.toString()
    );
    // await sleep(1000);
    for (let i = 0; i < 5; i++) {
      let amount_out = new BN(1_000000000);
      const tx = await swap_base_output(
        program,
        owner,
        configAddress,
        poolState.token0Mint,
        poolState.token0Program,
        poolState.token1Mint,
        poolState.token1Program,
        amount_out,
        new BN(10000000000000),
        confirmOptions,
        poolAddress
      );
    }

    for (let i = 0; i < 5; i++) {
      let amount_out = new BN(1_000000000);
      const tx = await swap_base_output(
        program,
        owner,
        configAddress,
        poolState.token1Mint,
        poolState.token1Program,
        poolState.token0Mint,
        poolState.token0Program,
        amount_out,
        new BN(10000000000000),
        confirmOptions,
        poolAddress
      );
    }

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

    const poolBeforeCollect = await program.account.poolState.fetch(
      poolAddress
    );
    assert.isTrue(poolBeforeCollect.lpFeesToken0.toNumber() > 0);
    assert.isTrue(poolBeforeCollect.lpFeesToken1.toNumber() > 0);
    const positionBeforeCollect = await program.account.position.fetch(
      position
    );
    assert.equal(positionBeforeCollect.feesToken0PerLpLast.toNumber(), 0);
    assert.equal(positionBeforeCollect.feesToken1PerLpLast.toNumber(), 0);
    await collectPositionFee(
      program,
      owner,
      poolAddress,
      positionNftMintAddress,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      confirmOptions
    );

    const token0AccountAfterCollect = await getAccount(
      anchor.getProvider().connection,
      token0AccountAddr,
      "processed",
      poolState.token0Program
    );

    const token1AccountAfterCollect = await getAccount(
      anchor.getProvider().connection,
      token1AccountAddr,
      "processed",
      poolState.token1Program
    );

    const token0AmountCollect =
      token0AccountAfterCollect.amount - token0AccountBefore.amount;

    const token1AmountCollect =
      token1AccountAfterCollect.amount - token1AccountBefore.amount;
    assert.isTrue(
      almostEqual(
        lpFee(
          lpAmount,
          poolBeforeCollect.lpSupply,
          poolBeforeCollect.lpFeesToken0
        ).toNumber(),
        Number(token0AmountCollect),
        5
      )
    );
    assert.isTrue(
      almostEqual(
        lpFee(
          lpAmount,
          poolBeforeCollect.lpSupply,
          poolBeforeCollect.lpFeesToken1
        ).toNumber(),
        Number(token1AmountCollect),
        5
      )
    );
    const poolAfterCollect = await program.account.poolState.fetch(poolAddress);
    const positionAfterCollect = await program.account.position.fetch(position);
    assert.equal(
      positionAfterCollect.feesToken0PerLpLast.toNumber(),
      poolAfterCollect.feesToken0PerLp.toNumber()
    );
    assert.equal(
      positionAfterCollect.feesToken1PerLpLast.toNumber(),
      poolAfterCollect.feesToken1PerLp.toNumber()
    );

    assert.equal(positionAfterCollect.feesOwedToken0.toNumber(), 0);
    assert.equal(positionAfterCollect.feesOwedToken1.toNumber(), 0);
  });
});

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function lpFee(lpAmount: BN, lpSupply: BN, feeTotal: BN): BN {
  return lpAmount.mul(feeTotal).div(lpSupply);
}

function almostEqual(a: number, b: number, epsilon = 5): boolean {
  return a - b < epsilon;
}
