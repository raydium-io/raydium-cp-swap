import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import {
  deposit,
  getUserAndPoolVaultAmount,
  isEqual,
  setupDepositTest,
  withdraw,
} from "./utils";
import { assert } from "chai";

describe("withdraw test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  it("withdraw half of lp ", async () => {
    const { poolAddress, poolState } = await setupDepositTest(
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
    const liquidity = new BN(10000000000);
    await deposit(
      program,
      owner,
      poolState.ammConfig,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      liquidity,
      new BN(10000000000),
      new BN(20000000000)
    );

    await withdraw(
      program,
      owner,
      poolState.ammConfig,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      liquidity.divn(2),
      new BN(10000000),
      new BN(1000000),
      confirmOptions
    );
    const newPoolState = await program.account.poolState.fetch(poolAddress);
    assert(newPoolState.lpSupply.eq(liquidity.divn(2).add(poolState.lpSupply)));
  });

  it("withdraw all lp ", async () => {
    const { poolAddress, poolState } = await setupDepositTest(
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
    const liquidity = new BN(10000000000);
    const {
      onwerToken0Account: ownerToken0AccountBefore,
      onwerToken1Account: ownerToken1AccountBefore,
      poolVault0TokenAccount: poolVault0TokenAccountBefore,
      poolVault1TokenAccount: poolVault1TokenAccountBefore,
    } = await getUserAndPoolVaultAmount(
      owner.publicKey,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      poolState.token0Vault,
      poolState.token1Vault
    );

    await deposit(
      program,
      owner,
      poolState.ammConfig,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      liquidity,
      new BN(10000000000),
      new BN(20000000000)
    );

    await withdraw(
      program,
      owner,
      poolState.ammConfig,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      liquidity,
      new BN(10000000),
      new BN(1000000),
      confirmOptions
    );

    const newPoolState = await program.account.poolState.fetch(poolAddress);
    assert(newPoolState.lpSupply.eq(poolState.lpSupply));

    const {
      onwerToken0Account: ownerToken0AccountAfter,
      onwerToken1Account: ownerToken1AccountAfter,
      poolVault0TokenAccount: poolVault0TokenAccountAfter,
      poolVault1TokenAccount: poolVault1TokenAccountAfter,
    } = await getUserAndPoolVaultAmount(
      owner.publicKey,
      poolState.token0Mint,
      poolState.token0Program,
      poolState.token1Mint,
      poolState.token1Program,
      poolState.token0Vault,
      poolState.token1Vault
    );

    assert(
      isEqual(ownerToken0AccountBefore.amount, ownerToken0AccountAfter.amount)
    );
    assert(
      isEqual(ownerToken1AccountBefore.amount, ownerToken1AccountAfter.amount)
    );
    assert(
      isEqual(
        poolVault0TokenAccountBefore.amount,
        poolVault0TokenAccountAfter.amount
      )
    );
    assert(
      isEqual(
        poolVault1TokenAccountBefore.amount,
        poolVault1TokenAccountAfter.amount
      )
    );
  });
});
