pub mod curve;
pub mod error;
pub mod instructions;
pub mod states;
pub mod utils;
use crate::curve::fees::FEE_RATE_DENOMINATOR_VALUE;
use anchor_lang::prelude::*;
use instructions::*;
pub use states::CreatorFeeOn;

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "raydium-cp-swap",
    project_url: "https://raydium.io",
    contacts: "link:https://immunefi.com/bounty/raydium",
    policy: "https://immunefi.com/bounty/raydium",
    source_code: "https://github.com/raydium-io/raydium-cp-swap",
    preferred_languages: "en",
    auditors: "https://github.com/raydium-io/raydium-docs/blob/master/audit/MadShield%20Q1%202024/raydium-cp-swap-v-1.0.0.pdf"
}

#[cfg(feature = "devnet")]
declare_id!("DRaycpLY18LhpbydsBWbVJtxpNv9oXPgjRSfpF2bWpYb");
#[cfg(not(feature = "devnet"))]
declare_id!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");

pub mod admin {
    #[cfg(not(feature = "localnet"))]
    use super::pubkey;
    use super::Pubkey;
    #[cfg(feature = "localnet")]
    pub const ID: Pubkey = Pubkey::from_str_const(env!(
        "CPSWAP_LOCALNET_ADMIN",
        "the `localnet` feature needs CPSWAP_LOCALNET_ADMIN=<admin pubkey> at build time (run `yarn test:local-admin`)"
    ));
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("DRayqG9RXYi8WHgWEmRQGrUWRWbhjYWYkCRJDd6JBBak");
    #[cfg(all(not(feature = "devnet"), not(feature = "localnet")))]
    pub const ID: Pubkey = pubkey!("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ");
}

pub mod create_pool_fee_reveiver {
    use super::{pubkey, Pubkey};
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("3oE58BKVt8KuYkGxx8zBojugnymWmBiyafWgMrnb6eYy");
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("DNXgeM9EiiaAbaWvwjHj9fQQLAX5ZsfHyvmYUNRAdNC8");
}

pub mod collect_lamports {
    use super::{pubkey, Pubkey};
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("DRaydJNq54dSDHUqYCE3G8YySgaXfZucbh7dTXw9fBMs");
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("RayGkhY93thaTgCv98sx1pNLgBHhJDxWUeZXp4bjmnp");
}

pub mod fund_fee_owner {
    use super::{pubkey, Pubkey};
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("DRay33UmULQCeawH3dVpJfN3uqLj6Qtq4ymSRx2pAgGK");
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("FUNDduJTA7XcckKHKfAoEnnhuSud2JUCUZv6opWEjrBU");
}

pub mod protocol_fee_owner {
    use super::{pubkey, Pubkey};
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("DRay33UmULQCeawH3dVpJfN3uqLj6Qtq4ymSRx2pAgGK");
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("ProCXqRcXJjoUd1RNoo28bSizAA6EEqt9wURZYPDc5u");
}

pub const AUTH_SEED: &str = "vault_and_lp_mint_auth_seed";

#[program]
pub mod raydium_cp_swap {
    use super::*;

    // The configuration of AMM protocol, include trade fee and protocol fee
    /// # Arguments
    ///
    /// * `ctx`- The accounts needed by instruction.
    /// * `index` - The index of amm config, there may be multiple config.
    /// * `trade_fee_rate` - Trade fee rate, can be changed.
    /// * `protocol_fee_rate` - The rate of protocol fee within trade fee.
    /// * `fund_fee_rate` - The rate of fund fee within trade fee.
    ///
    pub fn create_amm_config(
        ctx: Context<CreateAmmConfig>,
        index: u16,
        trade_fee_rate: u64,
        protocol_fee_rate: u64,
        fund_fee_rate: u64,
        create_pool_fee: u64,
        creator_fee_rate: u64,
    ) -> Result<()> {
        assert!(trade_fee_rate + creator_fee_rate < FEE_RATE_DENOMINATOR_VALUE);
        assert!(protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        assert!(fund_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        assert!(fund_fee_rate + protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        instructions::create_amm_config(
            ctx,
            index,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
            create_pool_fee,
            creator_fee_rate,
        )
    }

    /// Updates the owner of the amm config
    /// Must be called by the current owner or admin
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `trade_fee_rate`- The new trade fee rate of amm config, be set when `param` is 0
    /// * `protocol_fee_rate`- The new protocol fee rate of amm config, be set when `param` is 1
    /// * `fund_fee_rate`- The new fund fee rate of amm config, be set when `param` is 2
    /// * `new_owner`- The config's new owner, be set when `param` is 3
    /// * `new_fund_owner`- The config's new fund owner, be set when `param` is 4
    /// * `param`- The value can be 0 | 1 | 2 | 3 | 4, otherwise will report a error
    ///
    pub fn update_amm_config(ctx: Context<UpdateAmmConfig>, param: u8, value: u64) -> Result<()> {
        instructions::update_amm_config(ctx, param, value)
    }

    /// Update pool status for given value
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `status` - The value of status
    ///
    pub fn update_pool_status(ctx: Context<UpdatePoolStatus>, status: u8) -> Result<()> {
        instructions::update_pool_status(ctx, status)
    }

    /// Collect the protocol fee accrued to the pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `amount_0_requested` - The maximum amount of token_0 to send, can be 0 to collect fees in only token_1
    /// * `amount_1_requested` - The maximum amount of token_1 to send, can be 0 to collect fees in only token_0
    ///
    pub fn collect_protocol_fee(
        ctx: Context<CollectProtocolFee>,
        amount_0_requested: u64,
        amount_1_requested: u64,
    ) -> Result<()> {
        instructions::collect_protocol_fee(ctx, amount_0_requested, amount_1_requested)
    }

    /// Collect the fund fee accrued to the pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `amount_0_requested` - The maximum amount of token_0 to send, can be 0 to collect fees in only token_1
    /// * `amount_1_requested` - The maximum amount of token_1 to send, can be 0 to collect fees in only token_0
    ///
    pub fn collect_fund_fee(
        ctx: Context<CollectFundFee>,
        amount_0_requested: u64,
        amount_1_requested: u64,
    ) -> Result<()> {
        instructions::collect_fund_fee(ctx, amount_0_requested, amount_1_requested)
    }

    /// Collect the creator fee
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    ///
    pub fn collect_creator_fee(ctx: Context<CollectCreatorFee>) -> Result<()> {
        instructions::collect_creator_fee(ctx)
    }

    /// Collect the creator fee, anyone can call it since the fee is always sent to the
    /// pool creator, the payer only funds the creation of the creator's token accounts.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    ///
    pub fn collect_creator_fee_permissionless(
        ctx: Context<CollectCreatorFeePermissionless>,
    ) -> Result<()> {
        instructions::collect_creator_fee_permissionless(ctx)
    }

    /// Create a permission account
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    ///
    pub fn create_permission_pda(ctx: Context<CreatePermissionPda>) -> Result<()> {
        instructions::create_permission_pda(ctx)
    }

    /// Close a permission account
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    ///
    pub fn close_permission_pda(ctx: Context<ClosePermissionPda>) -> Result<()> {
        instructions::close_permission_pda(ctx)
    }

    /// Creates a pool for the given token pair and the initial price
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `init_amount_0` - the initial amount_0 to deposit
    /// * `init_amount_1` - the initial amount_1 to deposit
    /// * `open_time` - the timestamp allowed for swap
    ///
    pub fn initialize(
        ctx: Context<Initialize>,
        init_amount_0: u64,
        init_amount_1: u64,
        open_time: u64,
    ) -> Result<()> {
        instructions::initialize(ctx, init_amount_0, init_amount_1, open_time)
    }

    /// Create a pool with permission
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `init_amount_0` - the initial amount_0 to deposit
    /// * `init_amount_1` - the initial amount_1 to deposit
    /// * `open_time` - the timestamp allowed for swap
    /// * `creator_fee_on` - creator fee model, 0：both token0 and token1 (depends on the input), 1: only token0, 2: only token1
    ///
    pub fn initialize_with_permission(
        ctx: Context<InitializeWithPermission>,
        init_amount_0: u64,
        init_amount_1: u64,
        open_time: u64,
        creator_fee_on: CreatorFeeOn,
    ) -> Result<()> {
        instructions::initialize_with_permission(
            ctx,
            init_amount_0,
            init_amount_1,
            open_time,
            creator_fee_on,
        )
    }

    /// Deposit lp token to the pool
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `lp_token_amount` - Increased number of LPs
    /// * `maximum_token_0_amount` -  Maximum token 0 amount to deposit, prevents excessive slippage
    /// * `maximum_token_1_amount` - Maximum token 1 amount to deposit, prevents excessive slippage
    ///
    pub fn deposit(
        ctx: Context<Deposit>,
        lp_token_amount: u64,
        maximum_token_0_amount: u64,
        maximum_token_1_amount: u64,
    ) -> Result<()> {
        instructions::deposit(
            ctx,
            lp_token_amount,
            maximum_token_0_amount,
            maximum_token_1_amount,
        )
    }

    /// Withdraw lp for token0 and token1
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `lp_token_amount` - Amount of pool tokens to burn. User receives an output of token a and b based on the percentage of the pool tokens that are returned.
    /// * `minimum_token_0_amount` -  Minimum amount of token 0 to receive, prevents excessive slippage
    /// * `minimum_token_1_amount` -  Minimum amount of token 1 to receive, prevents excessive slippage
    ///
    pub fn withdraw(
        ctx: Context<Withdraw>,
        lp_token_amount: u64,
        minimum_token_0_amount: u64,
        minimum_token_1_amount: u64,
    ) -> Result<()> {
        instructions::withdraw(
            ctx,
            lp_token_amount,
            minimum_token_0_amount,
            minimum_token_1_amount,
        )
    }

    /// Swap the tokens in the pool base input amount
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `amount_in` -  input amount to transfer, output to DESTINATION is based on the exchange rate
    /// * `minimum_amount_out` -  Minimum amount of output token, prevents excessive slippage
    ///
    pub fn swap_base_input(
        ctx: Context<Swap>,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::swap_base_input(ctx, amount_in, minimum_amount_out)
    }

    /// Swap the tokens in the pool base output amount
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `max_amount_in` -  input amount prevents excessive slippage
    /// * `amount_out` -  amount of output token
    ///
    pub fn swap_base_output(ctx: Context<Swap>, max_amount_in: u64, amount_out: u64) -> Result<()> {
        instructions::swap_base_output(ctx, max_amount_in, amount_out)
    }

    /// Token collections
    ///
    /// A ruleset is an admin-defined admission rule (e.g. "standard non-mayhem pump.fun launch").
    /// Anyone may create a collection that points at a ruleset, and anyone may register a mint
    /// into a collection by supplying the accounts that prove the rule holds. Pools whose two mints
    /// are members of one collection gain a discounted `rebalance_swap_base_input` path.

    /// Create an admission ruleset. Admin only.
    ///
    /// * `index` - ruleset seed
    /// * `kind` - `RuleKind` (0 any, 1 pump.fun launch, 2 immutable mint, 3 stake-pool LST)
    /// * `flags` - kind-specific flags (pump.fun: 1 allow mayhem, 2 require complete)
    /// * `program_id` - program whose accounts prove the rule (pump.fun program for kind 1)
    pub fn create_ruleset(
        ctx: Context<CreateRuleset>,
        index: u16,
        kind: u8,
        flags: u8,
        program_id: Pubkey,
    ) -> Result<()> {
        instructions::create_ruleset(ctx, index, kind, flags, program_id)
    }

    /// Update an admission ruleset. Admin only.
    pub fn update_ruleset(
        ctx: Context<UpdateRuleset>,
        kind: u8,
        flags: u8,
        program_id: Pubkey,
    ) -> Result<()> {
        instructions::update_ruleset(ctx, kind, flags, program_id)
    }

    /// Create a token collection bound to a ruleset. Permissionless.
    ///
    /// * `index` - collection seed, scoped by the creator
    /// * `rebalance_fee_divisor` - rebalance swaps pay `trade_fee_rate / divisor`
    pub fn create_token_collection(
        ctx: Context<CreateTokenCollection>,
        index: u16,
        rebalance_fee_divisor: u32,
    ) -> Result<()> {
        instructions::create_token_collection(ctx, index, rebalance_fee_divisor)
    }

    /// Update a token collection. Collection authority only.
    ///
    /// * `param` - 0 rebalance_fee_divisor, 1 new authority (first remaining account)
    pub fn update_token_collection(
        ctx: Context<UpdateTokenCollection>,
        param: u8,
        value: u64,
    ) -> Result<()> {
        instructions::update_token_collection(ctx, param, value)
    }

    /// Register a mint into a collection after checking the ruleset against the supplied proof
    /// accounts (remaining accounts). Permissionless.
    pub fn register_collection_member<'info>(
        ctx: Context<'info, RegisterCollectionMember<'info>>,
    ) -> Result<()> {
        instructions::register_collection_member(ctx)
    }

    /// Set a member's value in the collection numeraire (1e9 == 1.0). Collection authority only.
    pub fn set_collection_member_rate(ctx: Context<SetCollectionMemberRate>, rate: u64) -> Result<()> {
        instructions::set_collection_member_rate(ctx, rate)
    }

    /// Re-read an LST member's rate from its stake pool (`Lst` rulesets). Permissionless.
    pub fn sync_member_rate(ctx: Context<SyncMemberRate>) -> Result<()> {
        instructions::sync_member_rate(ctx)
    }

    /// Swap base input at `trade_fee_rate / collection.rebalance_fee_divisor`. Both pool mints must be
    /// members of `collection`, and the swap must strictly reduce the pool's rate-weighted imbalance.
    ///
    /// * `amount_in` - input amount to transfer
    /// * `minimum_amount_out` - minimum output, prevents excessive slippage
    pub fn rebalance_swap_base_input(
        ctx: Context<RebalanceSwap>,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::rebalance_swap_base_input(ctx, amount_in, minimum_amount_out)
    }

    /// Collection pools
    ///
    /// A pair that contains a collection's quote mint can bind to that collection. Other members of
    /// the collection get their own vaults under the pool, and any two members (the pair's base token
    /// included) swap directly against each other inside the pool: `intra_swap`, priced by a
    /// StableSwap over the members' rate-normalised reserves, so the price is the collection rate at
    /// balance and bends as the pool skews. Fee is `trade_fee_rate / rebalance_fee_divisor`.
    /// The pair's own curve against the quote is unchanged (extra swaps).

    /// Bind a pair to a collection. Pool creator or admin. `amp` is the StableSwap amplification.
    pub fn init_pool_members(ctx: Context<InitPoolMembers>, amp: u64) -> Result<()> {
        instructions::init_pool_members(ctx, amp)
    }

    /// Add a collection member to the pool (creates its vault). Permissionless.
    pub fn add_pool_member(ctx: Context<AddPoolMember>) -> Result<()> {
        instructions::add_pool_member(ctx)
    }

    /// Swap member `from_index` for member `to_index` inside the pool.
    /// Remaining accounts: for every member k, `[member vault k, CollectionMember k]`.
    pub fn intra_swap<'info>(
        ctx: Context<'info, IntraSwap<'info>>,
        from_index: u8,
        to_index: u8,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::intra_swap(ctx, from_index, to_index, amount_in, minimum_amount_out)
    }

    /// Collect protocol (kind 0) or fund (kind 1) fees from a non-base member vault.
    pub fn collect_member_fees(ctx: Context<CollectMemberFees>, member_index: u8, kind: u8) -> Result<()> {
        instructions::collect_member_fees(ctx, member_index, kind)
    }

    /// Create support token22 mint account which can create pool and send rewards while ignoring unsupported extensions.
    pub fn create_support_mint_associated(ctx: Context<CreateSupportMintAssociated>) -> Result<()> {
        instructions::create_support_mint_associated(ctx)
    }

    /// Close support token22 mint account which can create pool and send rewards while ignoring unsupported extensions.
    pub fn close_support_mint_associated(ctx: Context<CloseSupportMintAssociated>) -> Result<()> {
        instructions::close_support_mint_associated(ctx)
    }

    /// Collect excess lamports, including accounts for SPL tokens owned by authority and Program PDA accounts.
    pub fn collect_excess_lamports<'info>(
        ctx: Context<'info, CollectExcessLamports<'info>>,
    ) -> Result<()> {
        instructions::collect_excess_lamports(ctx)
    }
}
