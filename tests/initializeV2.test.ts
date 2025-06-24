import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";
import { getAccount, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { setupInitializeTest, calculateFee, initializeV2 } from "./utils";
import { assert } from "chai";

describe("initializeV2 test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  console.log("owner: ", owner.publicKey.toString());

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;
  const connection = anchor.getProvider().connection;
  const confirmOptions = {
    skipPreflight: true,
  };

  it("create pool with token2022 mint has transfer fee", async () => {
    const transferFeeConfig = { transferFeeBasisPoints: 100, MaxFee: 50000000 }; // %10
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
          create_fee: new BN(100000000),
        },
        transferFeeConfig,
        confirmOptions
      );

    const initAmount0 = new BN(10000000000);
    const initAmount1 = new BN(10000000000);
    const { poolAddress, poolState } = await initializeV2(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      0,
      confirmOptions,
      { initAmount0, initAmount1 }
    );
    let vault0 = await getAccount(
      connection,
      poolState.token0Vault,
      "processed",
      poolState.token0Program
    );
    if (token0Program == TOKEN_PROGRAM_ID) {
      assert.equal(vault0.amount.toString(), initAmount0.toString());
    } else {
      const total =
        vault0.amount +
        calculateFee(
          transferFeeConfig,
          BigInt(initAmount0.toString()),
          poolState.token0Program
        );
      assert(new BN(total.toString()).gte(initAmount0));
    }

    let vault1 = await getAccount(
      connection,
      poolState.token1Vault,
      "processed",
      poolState.token1Program
    );
    if (token1Program == TOKEN_PROGRAM_ID) {
      assert.equal(vault1.amount.toString(), initAmount1.toString());
    } else {
      const total =
        vault1.amount +
        calculateFee(
          transferFeeConfig,
          BigInt(initAmount1.toString()),
          poolState.token1Program
        );
      assert(new BN(total.toString()).gte(initAmount1));
    }
  });
});
