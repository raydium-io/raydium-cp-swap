import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import {
  calculateFee,
  calculatePreFeeAmount,
  deposit,
  getUserAndPoolVaultAmount,
  setupDepositTest,
} from "./utils";
import { assert } from "chai";
import { MAX_FEE_BASIS_POINTS, TOKEN_PROGRAM_ID } from "@solana/spl-token";

describe("deposit test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  it("deposit test, add the same liquidity and check the correctness of the values with and without transfer fees", async () => {
    /// deposit without fee
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
      new BN(20000000000),
      confirmOptions
    );
    const newPoolState = await program.account.poolState.fetch(poolAddress);
    assert(newPoolState.lpSupply.eq(liquidity.add(poolState.lpSupply)));

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
    const input_token0_amount =
      ownerToken0AccountBefore.amount - ownerToken0AccountAfter.amount;
    const input_token1_amount =
      ownerToken1AccountBefore.amount - ownerToken1AccountAfter.amount;
    assert.equal(
      poolVault0TokenAccountAfter.amount - poolVault0TokenAccountBefore.amount,
      input_token0_amount
    );
    assert.equal(
      poolVault1TokenAccountAfter.amount - poolVault1TokenAccountBefore.amount,
      input_token1_amount
    );

    /// deposit with fee
    const transferFeeConfig = {
      transferFeeBasisPoints: 100,
      MaxFee: 50000000000,
    }; // %10

    // Ensure that the initialization state is the same with depsoit without fee
    const { poolAddress: poolAddress2, poolState: poolState2 } =
      await setupDepositTest(
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
        confirmOptions,
        {
          initAmount0: new BN(
            calculatePreFeeAmount(
              transferFeeConfig,
              poolVault0TokenAccountBefore.amount,
              poolState.token0Program
            ).toString()
          ),
          initAmount1: new BN(
            calculatePreFeeAmount(
              transferFeeConfig,
              poolVault1TokenAccountBefore.amount,
              poolState.token1Program
            ).toString()
          ),
        },
        {
          token0Program: poolState.token0Program,
          token1Program: poolState.token1Program,
        }
      );
    const {
      onwerToken0Account: onwerToken0AccountBefore2,
      onwerToken1Account: onwerToken1AccountBefore2,
      poolVault0TokenAccount: poolVault0TokenAccountBefore2,
      poolVault1TokenAccount: poolVault1TokenAccountBefore2,
    } = await getUserAndPoolVaultAmount(
      owner.publicKey,
      poolState2.token0Mint,
      poolState2.token0Program,
      poolState2.token1Mint,
      poolState2.token1Program,
      poolState2.token0Vault,
      poolState2.token1Vault
    );
    // check vault init state
    assert.equal(
      poolVault0TokenAccountBefore2.amount,
      poolVault0TokenAccountBefore.amount
    );
    assert.equal(
      poolVault1TokenAccountBefore2.amount,
      poolVault1TokenAccountBefore.amount
    );

    await deposit(
      program,
      owner,
      poolState2.ammConfig,
      poolState2.token0Mint,
      poolState2.token0Program,
      poolState2.token1Mint,
      poolState2.token1Program,
      liquidity,
      new BN(100000000000),
      new BN(200000000000),
      confirmOptions
    );
    const newPoolState2 = await program.account.poolState.fetch(poolAddress2);
    assert(newPoolState2.lpSupply.eq(liquidity.add(poolState2.lpSupply)));

    const {
      onwerToken0Account: onwerToken0AccountAfter2,
      onwerToken1Account: onwerToken1AccountAfter2,
      poolVault0TokenAccount: poolVault0TokenAccountAfter2,
      poolVault1TokenAccount: poolVault1TokenAccountAfter2,
    } = await getUserAndPoolVaultAmount(
      owner.publicKey,
      poolState2.token0Mint,
      poolState2.token0Program,
      poolState2.token1Mint,
      poolState2.token1Program,
      poolState2.token0Vault,
      poolState2.token1Vault
    );

    const input_token0_amount_with_fee =
      onwerToken0AccountBefore2.amount - onwerToken0AccountAfter2.amount;
    const input_token1_amount_with_fee =
      onwerToken1AccountBefore2.amount - onwerToken1AccountAfter2.amount;
    assert(input_token0_amount_with_fee >= input_token0_amount);
    assert(input_token1_amount_with_fee >= input_token1_amount);

    assert.equal(
      input_token0_amount_with_fee,
      calculateFee(
        transferFeeConfig,
        input_token0_amount_with_fee,
        poolState2.token0Program
      ) + input_token0_amount
    );
    assert.equal(
      input_token1_amount_with_fee,
      calculateFee(
        transferFeeConfig,
        input_token1_amount_with_fee,
        poolState2.token1Program
      ) + input_token1_amount
    );

    // Add the same liquidity, the amount increment of the pool vault will be the same as without fees.
    assert.equal(
      poolVault0TokenAccountAfter2.amount -
        poolVault0TokenAccountBefore2.amount,
      input_token0_amount
    );
    assert.equal(
      poolVault1TokenAccountAfter2.amount -
        poolVault1TokenAccountBefore2.amount,
      input_token1_amount
    );

    assert.equal(
      poolVault0TokenAccountAfter.amount,
      poolVault0TokenAccountAfter2.amount
    );
    assert.equal(
      poolVault1TokenAccountAfter.amount,
      poolVault1TokenAccountAfter2.amount
    );
  });

  it("deposit test with 100% transferFeeConfig, reache maximum fee limit", async () => {
    const transferFeeConfig = {
      transferFeeBasisPoints: MAX_FEE_BASIS_POINTS,
      MaxFee: 5000000000,
    }; // %100

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
      transferFeeConfig
    );

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
      new BN(20000000000),
      confirmOptions
    );
    const newPoolState = await program.account.poolState.fetch(poolAddress);
    assert(newPoolState.lpSupply.eq(liquidity.add(poolState.lpSupply)));

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
    const input_token0_amount =
      ownerToken0AccountBefore.amount - ownerToken0AccountAfter.amount;
    const input_token1_amount =
      ownerToken1AccountBefore.amount - ownerToken1AccountAfter.amount;

    if (poolState.token0Program.equals(TOKEN_PROGRAM_ID)) {
      assert.equal(
        poolVault0TokenAccountAfter.amount -
          poolVault0TokenAccountBefore.amount,
        input_token0_amount
      );
      assert.equal(
        poolVault1TokenAccountAfter.amount -
          poolVault1TokenAccountBefore.amount,
        input_token1_amount - BigInt(transferFeeConfig.MaxFee)
      );
    } else {
      assert.equal(
        poolVault0TokenAccountAfter.amount -
          poolVault0TokenAccountBefore.amount,
        input_token0_amount - BigInt(transferFeeConfig.MaxFee)
      );
      assert.equal(
        poolVault1TokenAccountAfter.amount -
          poolVault1TokenAccountBefore.amount,
        input_token1_amount
      );
    }
  });
});
