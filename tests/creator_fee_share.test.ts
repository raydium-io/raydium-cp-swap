import * as anchor from "@anchor-lang/core";
import { Program, BN } from "@anchor-lang/core";
import { RaydiumCpSwap } from "../target/types/raydium_cp_swap";

import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import {
  accountExist,
  closeCreatorFeeShare,
  createAmmConfig,
  createCreatorFeeShare,
  sendTransaction,
  updateAmmConfig,
} from "./utils";
import { assert } from "chai";
import { SystemProgram } from "@solana/web3.js";

const FEE_RATE_DENOMINATOR = new BN(1_000_000);
const CREATOR_FEE_SHARE_RATE_PARAM = 8;

describe("creator fee share test", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const owner = anchor.Wallet.local().payer;
  const connection = anchor.getProvider().connection;
  const program = anchor.workspace.RaydiumCpSwap as Program<RaydiumCpSwap>;

  const confirmOptions = { skipPreflight: true };

  // Everything here is admin gated: creating an amm config and the share account both
  // check `crate::admin::ID`. `yarn test:local-admin` builds with `--features localnet`
  // and compiles the local wallet in as admin; on a stock build the suite is skipped.
  const adminIsLocalWallet =
    process.env.CPSWAP_LOCALNET_ADMIN === owner.publicKey.toBase58();

  const creator = Keypair.generate();
  const outsider = Keypair.generate();
  let configAddress: PublicKey;

  before(async function () {
    if (!adminIsLocalWallet) {
      this.skip();
    }
    this.timeout(300000);

    await sendTransaction(
      connection,
      [
        SystemProgram.transfer({
          fromPubkey: owner.publicKey,
          toPubkey: creator.publicKey,
          lamports: LAMPORTS_PER_SOL,
        }),
        SystemProgram.transfer({
          fromPubkey: owner.publicKey,
          toPubkey: outsider.publicKey,
          lamports: LAMPORTS_PER_SOL,
        }),
      ],
      [owner]
    );

    configAddress = await createAmmConfig(
      program,
      connection,
      owner,
      880,
      new BN(25000),
      new BN(120000),
      new BN(40000),
      new BN(0),
      new BN(25000),
      confirmOptions
    );
  });

  it("a share rate above the fee denominator is rejected", async () => {
    try {
      await createCreatorFeeShare(
        program,
        owner,
        outsider.publicKey,
        configAddress,
        FEE_RATE_DENOMINATOR.addn(1)
      );
      assert.fail("an out of range share rate must be rejected");
    } catch (e) {
      assert.include(String(e), "InvalidInput");
    }
  });

  it("a non admin cannot create or close a share account", async () => {
    try {
      await createCreatorFeeShare(
        program,
        outsider,
        creator.publicKey,
        configAddress,
        new BN(500000)
      );
      assert.fail("a non admin must not create a share account");
    } catch (e) {
      assert.include(String(e), "InvalidOwner");
    }

    const shareAddress = await createCreatorFeeShare(
      program,
      owner,
      creator.publicKey,
      configAddress,
      new BN(500000),
      confirmOptions
    );
    try {
      await closeCreatorFeeShare(
        program,
        outsider,
        creator.publicKey,
        configAddress
      );
      assert.fail("a non admin must not close a share account");
    } catch (e) {
      assert.include(String(e), "InvalidOwner");
    }
    assert.isTrue(await accountExist(connection, shareAddress));
    await closeCreatorFeeShare(
      program,
      owner,
      creator.publicKey,
      configAddress,
      confirmOptions
    );
  });

  it("rejects a config share rate above the fee denominator", async () => {
    // the handler asserts, so this surfaces as a program panic rather than an error code
    let rejected = false;
    try {
      await updateAmmConfig(
        program,
        owner,
        configAddress,
        CREATOR_FEE_SHARE_RATE_PARAM,
        FEE_RATE_DENOMINATOR.addn(1)
      );
    } catch {
      rejected = true;
    }
    assert.isTrue(
      rejected,
      "an out of range config share rate must be rejected"
    );
    const ammConfig = await program.account.ammConfig.fetch(configAddress);
    assert.isTrue(ammConfig.creatorFeeShareRate.lte(FEE_RATE_DENOMINATOR));
  });
});
