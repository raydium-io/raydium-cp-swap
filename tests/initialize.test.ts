import * as anchor from "@anchor-lang/core";
import { Program, BN } from "@anchor-lang/core";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";

import { getAccount, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { setupInitializeTest, initialize, calculateFee } from "./utils";
import { assert } from "chai";

describe("initialize test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  console.log("owner: ", owner.publicKey.toString());

  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = {
    skipPreflight: true,
  };

  // the create-pool fee is transferred to this fixed wsol account
  // (crate::create_pool_fee_reveiver::ID) and synced into its wrapped amount
  const createPoolFeeReceiver = new PublicKey(
    "DNXgeM9EiiaAbaWvwjHj9fQQLAX5ZsfHyvmYUNRAdNC8"
  );

  // creating an amm config is admin-gated, `yarn test:local-admin` compiles the
  // local wallet in as admin; without it there is no way to get a config that
  // differs from the mainnet ones cloned into the validator
  const adminIsLocalWallet =
    process.env.CPSWAP_LOCALNET_ADMIN === owner.publicKey.toBase58();

  async function getCollectedCreatePoolFee() {
    const account = await getAccount(
      anchor.getProvider().connection,
      createPoolFeeReceiver,
      "processed",
      TOKEN_PROGRAM_ID
    );
    return account.amount;
  }

  it("create pool without fee", async function () {
    // amm config 0 is cloned from mainnet and carries a real create_pool_fee, so
    // the zero-fee path needs a config of its own; index 1000 is unused onchain
    if (!adminIsLocalWallet) this.skip();
    const { configAddress, token0, token0Program, token1, token1Program } =
      await setupInitializeTest(
        program,
        anchor.getProvider().connection,
        owner,
        {
          config_index: 1000,
          tradeFeeRate: new BN(10),
          protocolFeeRate: new BN(1000),
          fundFeeRate: new BN(25000),
          create_fee: new BN(0),
        },
        { transferFeeBasisPoints: 0, MaxFee: 0 },
        confirmOptions
      );
    const ammConfig = await program.account.ammConfig.fetch(configAddress);
    assert.equal(ammConfig.createPoolFee.toString(), "0");
    const collectedFeeBefore = await getCollectedCreatePoolFee();

    const initAmount0 = new BN(10000000000);
    const initAmount1 = new BN(10000000000);
    const { poolAddress, poolState } = await initialize(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions,
      { initAmount0, initAmount1 }
    );
    let vault0 = await getAccount(
      anchor.getProvider().connection,
      poolState.token0Vault,
      "processed",
      poolState.token0Program
    );
    assert.equal(vault0.amount.toString(), initAmount0.toString());

    let vault1 = await getAccount(
      anchor.getProvider().connection,
      poolState.token1Vault,
      "processed",
      poolState.token1Program
    );
    assert.equal(vault1.amount.toString(), initAmount1.toString());
    assert.equal(
      (await getCollectedCreatePoolFee()).toString(),
      collectedFeeBefore.toString(),
      "a config with create_pool_fee = 0 must not charge the creator"
    );
  });

  it("create pool with fee", async () => {
    const { configAddress, token0, token0Program, token1, token1Program } =
      await setupInitializeTest(
        program,
        anchor.getProvider().connection,
        owner,
        {
          config_index: 0,
          tradeFeeRate: new BN(10),
          protocolFeeRate: new BN(1000),
          fundFeeRate: new BN(25000),
          // only used if config 0 has to be created, the cloned mainnet config
          // already carries its own create_pool_fee
          create_fee: new BN(100000000),
        },
        { transferFeeBasisPoints: 0, MaxFee: 0 },
        confirmOptions
      );
    const ammConfig = await program.account.ammConfig.fetch(configAddress);
    assert(
      ammConfig.createPoolFee.gtn(0),
      "amm config 0 must carry a create fee for this case to mean anything"
    );
    const collectedFeeBefore = await getCollectedCreatePoolFee();

    const initAmount0 = new BN(10000000000);
    const initAmount1 = new BN(10000000000);
    const { poolAddress, poolState } = await initialize(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions,
      { initAmount0, initAmount1 }
    );
    let vault0 = await getAccount(
      anchor.getProvider().connection,
      poolState.token0Vault,
      "processed",
      poolState.token0Program
    );
    assert.equal(vault0.amount.toString(), initAmount0.toString());

    let vault1 = await getAccount(
      anchor.getProvider().connection,
      poolState.token1Vault,
      "processed",
      poolState.token1Program
    );
    assert.equal(vault1.amount.toString(), initAmount1.toString());
    assert.equal(
      ((await getCollectedCreatePoolFee()) - collectedFeeBefore).toString(),
      ammConfig.createPoolFee.toString(),
      "the create-pool fee receiver must collect exactly the configured fee"
    );
  });

  it("create pool with token2022 mint has transfer fee", async () => {
    const transferFeeConfig = { transferFeeBasisPoints: 100, MaxFee: 50000000 }; // %10
    const { configAddress, token0, token0Program, token1, token1Program } =
      await setupInitializeTest(
        program,
        anchor.getProvider().connection,
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
    const { poolAddress, poolState } = await initialize(
      program,
      owner,
      configAddress,
      token0,
      token0Program,
      token1,
      token1Program,
      confirmOptions,
      { initAmount0, initAmount1 }
    );
    let vault0 = await getAccount(
      anchor.getProvider().connection,
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
      anchor.getProvider().connection,
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
