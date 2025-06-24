use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::Token2022,
    token_interface::{Mint, TokenAccount},
};
use std::ops::DerefMut;
#[event_cpi]
#[derive(Accounts)]
pub struct ClosePosition<'info> {
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
        close = position_nft_owner
    )]
    pub position: Account<'info, Position>,

    /// Mint address bound to the personal position.
    #[account(
        mut,
        address = position.nft_mint,
        mint::token_program = token_program,
    )]
    pub position_nft_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        address = position.pool_id
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Spl token program 2022
    pub token_program: Program<'info, Token2022>,

    /// System program to close the position state account
    pub system_program: Program<'info, System>,
}

pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.load()?;

    let positon = ctx.accounts.position.deref_mut();
    positon.update_fee(
        pool_state.fees_token_0_per_lp,
        pool_state.fees_token_1_per_lp,
    )?;
    if positon.lp_amount > 0 || positon.fees_owed_token_0 > 0 || positon.fees_owed_token_1 > 0 {
        return err!(ErrorCode::NotApproved);
    }
    let position_nft_owner = ctx.accounts.position_nft_owner.to_account_info();
    let position_nft_mint = ctx.accounts.position_nft_mint.to_account_info();
    let personal_nft_account = ctx.accounts.position_nft_account.to_account_info();
    let token_program = ctx.accounts.token_program.to_account_info();
    token_burn(
        position_nft_owner.clone(),
        token_program.clone(),
        position_nft_mint.clone(),
        personal_nft_account.clone(),
        1,
        &[],
    )?;

    // close use nft token account
    close_spl_account(
        position_nft_owner.clone(),
        position_nft_owner.clone(),
        personal_nft_account,
        token_program.clone(),
        &[],
    )?;

    // close nft mint account
    close_spl_account(
        ctx.accounts.position.to_account_info(),
        position_nft_owner,
        position_nft_mint,
        token_program,
        &[&ctx.accounts.position.seeds()],
    )?;

    Ok(())
}
