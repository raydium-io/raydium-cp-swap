use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use std::ops::DerefMut;
#[event_cpi]
#[derive(Accounts)]
pub struct CollectPositionFee<'info> {
    /// Owner of position
    pub position_nft_owner: Signer<'info>,

    /// The token account for nft
    #[account(
        constraint = position_nft_account.mint == position.nft_mint,
        constraint = position_nft_account.amount == 1,
        token::authority = position_nft_owner,
    )]
    pub position_nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Position account
    #[account(
        mut,
        seeds = [
            POSITION_SEED.as_bytes(),
            position.nft_mint.as_ref()
        ],
        bump,
    )]
    pub position: Account<'info, Position>,

    /// CHECK: pool vault authority
    #[account(
        seeds = [
            crate::AUTH_SEED_V2.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        mut,
        address = position.pool_id
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The token account for receive token_0
    #[account(
        mut,
        token::mint = vault_0_mint,
    )]
    pub recipient_token_0_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token account for receive token_1
    #[account(
        mut,
        token::mint = vault_1_mint,
    )]
    pub recipient_token_1_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        token::mint = vault_0_mint,
        constraint = token_0_vault.key() == pool_state.load()?.token_0_vault
    )]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        token::mint = vault_1_mint,
        constraint = token_1_vault.key() == pool_state.load()?.token_1_vault
    )]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The mint of token_0 vault
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token_1 vault
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Spl token program or token program 2022
    pub token_0_program: Interface<'info, TokenInterface>,
    /// Spl token program or token program 2022
    pub token_1_program: Interface<'info, TokenInterface>,
}

pub fn collect_position_fee(ctx: Context<CollectPositionFee>) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;

    let positon = ctx.accounts.position.deref_mut();
    positon.update_fee(
        pool_state.fees_token_0_per_lp,
        pool_state.fees_token_1_per_lp,
    )?;
    if positon.fees_owed_token_0 == 0 && positon.fees_owed_token_1 == 0 {
        return err!(ErrorCode::CollectFeeZero);
    }
    if positon.fees_owed_token_0 > 0 {
        transfer_from_pool_vault_to_user(
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.token_0_vault.to_account_info(),
            ctx.accounts.recipient_token_0_account.to_account_info(),
            ctx.accounts.vault_0_mint.to_account_info(),
            ctx.accounts.token_0_program.to_account_info(),
            positon.fees_owed_token_0,
            ctx.accounts.vault_0_mint.decimals,
            &[&[crate::AUTH_SEED_V2.as_bytes(), &[ctx.bumps.authority]]],
        )?;
    }

    if positon.fees_owed_token_1 > 0 {
        transfer_from_pool_vault_to_user(
            ctx.accounts.authority.to_account_info(),
            ctx.accounts.token_1_vault.to_account_info(),
            ctx.accounts.recipient_token_1_account.to_account_info(),
            ctx.accounts.vault_1_mint.to_account_info(),
            ctx.accounts.token_1_program.to_account_info(),
            positon.fees_owed_token_1,
            ctx.accounts.vault_1_mint.decimals,
            &[&[crate::AUTH_SEED_V2.as_bytes(), &[ctx.bumps.authority]]],
        )?;
    }

    pool_state.lp_fees_token_0 = pool_state
        .lp_fees_token_0
        .checked_sub(positon.fees_owed_token_0)
        .unwrap();

    pool_state.lp_fees_token_1 = pool_state
        .lp_fees_token_1
        .checked_sub(positon.fees_owed_token_1)
        .unwrap();

    positon.fees_owed_token_0 = 0;
    positon.fees_owed_token_1 = 0;
    Ok(())
}
