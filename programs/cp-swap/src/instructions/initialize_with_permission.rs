use crate::instructions::initialize::{initialize_pool, InitializePoolAccounts};
use crate::states::*;
use anchor_lang::{accounts::interface_account::InterfaceAccount, prelude::*};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct InitializeWithPermission<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: creator of pool
    pub creator: UncheckedAccount<'info>,

    /// Which config the pool belongs to.
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// CHECK:
    /// pool vault and lp mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// CHECK: Initialize an account to store the pool state
    /// PDA account:
    /// seeds = [
    ///     POOL_SEED.as_bytes(),
    ///     amm_config.key().as_ref(),
    ///     token_0_mint.key().as_ref(),
    ///     token_1_mint.key().as_ref(),
    /// ],
    ///
    /// Or random account: must be signed by cli
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// Token_0 mint, the key must smaller than token_1 mint.
    #[account(
        constraint = token_0_mint.key() < token_1_mint.key(),
        mint::token_program = token_0_program,
    )]
    pub token_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Token_1 mint, the key must grater then token_0 mint.
    #[account(
        mint::token_program = token_1_program,
    )]
    pub token_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// pool lp mint
    #[account(
        init,
        seeds = [
            POOL_LP_MINT_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        mint::decimals = 9,
        mint::authority = authority,
        payer = payer,
        mint::token_program = token_program,
    )]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    /// payer token0 account
    #[account(
        mut,
        token::mint = token_0_mint,
        token::authority = payer,
    )]
    pub payer_token_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// payer token1 account
    #[account(
        mut,
        token::mint = token_1_mint,
        token::authority = payer,
    )]
    pub payer_token_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// payer lp token account
    #[account(
        init,
        associated_token::mint = lp_mint,
        associated_token::authority = payer,
        payer = payer,
        token::token_program = token_program,
    )]
    pub payer_lp_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Token_0 vault for the pool, created by contract
    #[account(
        mut,
        seeds = [
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_0_mint.key().as_ref()
        ],
        bump,
    )]
    pub token_0_vault: UncheckedAccount<'info>,

    /// CHECK: Token_1 vault for the pool, created by contract
    #[account(
        mut,
        seeds = [
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_1_mint.key().as_ref()
        ],
        bump,
    )]
    pub token_1_vault: UncheckedAccount<'info>,

    /// create pool fee account
    #[account(
        mut,
        address= crate::create_pool_fee_reveiver::ID,
    )]
    pub create_pool_fee: Box<InterfaceAccount<'info, TokenAccount>>,

    /// an account to store oracle observations
    #[account(
        init,
        seeds = [
            OBSERVATION_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = payer,
        space = ObservationState::LEN
    )]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// CHECK: PDA account used for permission verification.
    #[account(
        seeds = [
            PERMISSION_SEED.as_bytes(),
            payer.key().as_ref(),
        ],
        bump,
    )]
    pub permission: Box<Account<'info, Permission>>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    /// Spl token program or token program 2022
    pub token_0_program: Interface<'info, TokenInterface>,
    /// Spl token program or token program 2022
    pub token_1_program: Interface<'info, TokenInterface>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
    // remaining account
    // #[account(
    //     seeds = [
    //     SUPPORT_MINT_SEED.as_bytes(),
    //     token_0_mint.key().as_ref(),
    // ],
    //     bump
    // )]
    // pub support_mint0_associated: Account<'info, SupportMintAssociated>,

    // #[account(
    //     seeds = [
    //     SUPPORT_MINT_SEED.as_bytes(),
    //     token_1_mint.key().as_ref(),
    // ],
    //     bump
    // )]
    // pub support_mint1_associated: Account<'info, SupportMintAssociated>,
}

impl<'info> InitializeWithPermission<'info> {
    /// The view of these accounts that the shared pool creation flow works on: the
    /// `payer` funds the pool while the separate `creator` is recorded as its creator.
    fn pool_accounts<'a>(
        &'a self,
        remaining_accounts: &'a [AccountInfo<'info>],
        bumps: &InitializeWithPermissionBumps,
    ) -> InitializePoolAccounts<'a, 'info> {
        InitializePoolAccounts {
            payer: &self.payer,
            pool_creator: self.creator.key(),
            amm_config: &self.amm_config,
            authority: &self.authority,
            pool_state: &self.pool_state,
            token_0_mint: &self.token_0_mint,
            token_1_mint: &self.token_1_mint,
            lp_mint: &self.lp_mint,
            payer_token_0: &self.payer_token_0,
            payer_token_1: &self.payer_token_1,
            payer_lp_token: &self.payer_lp_token,
            token_0_vault: &self.token_0_vault,
            token_1_vault: &self.token_1_vault,
            create_pool_fee: &self.create_pool_fee,
            observation_state: &self.observation_state,
            token_program: &self.token_program,
            token_0_program: &self.token_0_program,
            token_1_program: &self.token_1_program,
            system_program: &self.system_program,
            remaining_accounts,
            authority_bump: bumps.authority,
            token_0_vault_bump: bumps.token_0_vault,
            token_1_vault_bump: bumps.token_1_vault,
        }
    }
}

pub fn initialize_with_permission<'info>(
    ctx: Context<'info, InitializeWithPermission<'info>>,
    init_amount_0: u64,
    init_amount_1: u64,
    open_time: u64,
    creator_fee_on: CreatorFeeOn,
) -> Result<()> {
    initialize_pool(
        ctx.accounts
            .pool_accounts(ctx.remaining_accounts, &ctx.bumps),
        init_amount_0,
        init_amount_1,
        open_time,
        true,
        creator_fee_on,
    )
}
