use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::Mint;
use anchor_spl::token_interface::TokenAccount;
use anchor_spl::token_interface::TokenInterface;
#[derive(Accounts)]
pub struct CollectCreatorFee<'info> {
    /// Only pool creator can collect fee
    #[account(mut, address = pool_state.load()?.pool_creator)]
    pub creator: Signer<'info>,

    /// CHECK: pool vault and lp mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Pool state stores accumulated protocol fee amount
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Amm config account stores fund_owner
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Account<'info, AmmConfig>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_0_vault.key() == pool_state.load()?.token_0_vault
    )]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_1_vault.key() == pool_state.load()?.token_1_vault
    )]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The mint of token_0 vault
    #[account(
        address = token_0_vault.mint
    )]
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token_1 vault
    #[account(
        address = token_1_vault.mint
    )]
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The address that receives the collected token_0 fund fees
    #[account(
        init_if_needed,
        associated_token::mint = vault_0_mint,
        associated_token::authority = creator,
        payer = creator,
        associated_token::token_program = token_0_program,
    )]
    pub creator_token_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that receives the collected token_1 fund fees
    #[account(
        init_if_needed,
        associated_token::mint = vault_1_mint,
        associated_token::authority = creator,
        payer = creator,
        associated_token::token_program = token_1_program,
    )]
    pub creator_token_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Spl token program or token program 2022
    pub token_0_program: Interface<'info, TokenInterface>,
    /// Spl token program or token program 2022
    pub token_1_program: Interface<'info, TokenInterface>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn collect_creator_fee(ctx: Context<CollectCreatorFee>) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let creator_fees_token_0 = pool_state.creator_fees_token_0;
    let creator_fees_token_1 = pool_state.creator_fees_token_1;
    if creator_fees_token_0 == 0 && creator_fees_token_1 == 0 {
        return err!(ErrorCode::NoFeeCollect);
    }

    let signer_seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]];

    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.token_0_vault.to_account_info(),
        ctx.accounts.creator_token_0.to_account_info(),
        ctx.accounts.vault_0_mint.to_account_info(),
        ctx.accounts.token_0_program.to_account_info(),
        creator_fees_token_0,
        ctx.accounts.vault_0_mint.decimals,
        &[signer_seeds],
    )?;

    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.token_1_vault.to_account_info(),
        ctx.accounts.creator_token_1.to_account_info(),
        ctx.accounts.vault_1_mint.to_account_info(),
        ctx.accounts.token_1_program.to_account_info(),
        creator_fees_token_1,
        ctx.accounts.vault_1_mint.decimals,
        &[signer_seeds],
    )?;

    pool_state.creator_fees_token_0 = 0;
    pool_state.creator_fees_token_1 = 0;
    pool_state.recent_epoch = Clock::get()?.epoch;

    Ok(())
}
