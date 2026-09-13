use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

/// Collects protocol (kind 0) or fund (kind 1) fees accrued in a non-base member vault.
#[derive(Accounts)]
pub struct CollectMemberFees<'info> {
    #[account(
        constraint = (owner.key() == amm_config.protocol_owner || owner.key() == amm_config.fund_owner || owner.key() == crate::admin::ID) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// CHECK: pool vault authority
    #[account(seeds = [crate::AUTH_SEED.as_bytes()], bump)]
    pub authority: UncheckedAccount<'info>,

    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [POOL_MEMBERS_SEED.as_bytes(), pool_state.key().as_ref()],
        bump = pool_members.bump,
    )]
    pub pool_members: Box<Account<'info, PoolMembers>>,

    #[account(mut)]
    pub member_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = member_vault.mint)]
    pub recipient_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = member_vault.mint)]
    pub vault_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn collect_member_fees(ctx: Context<CollectMemberFees>, member_index: u8, kind: u8) -> Result<()> {
    let idx = member_index as usize;
    let pm = &mut ctx.accounts.pool_members;
    require!(idx > 0 && idx < pm.n as usize, ErrorCode::InvalidPoolMember);
    require_keys_eq!(pm.members[idx].vault, ctx.accounts.member_vault.key(), ErrorCode::InvalidPoolMember);
    let amount = match kind {
        0 => std::mem::take(&mut pm.members[idx].protocol_fees_owed),
        1 => std::mem::take(&mut pm.members[idx].fund_fees_owed),
        _ => return err!(ErrorCode::InvalidInput),
    };
    require_gt!(amount, 0, ErrorCode::NoFeeCollect);
    let auth_bump = ctx.accounts.pool_state.load()?.auth_bump;
    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.member_vault.to_account_info(),
        ctx.accounts.recipient_token_account.to_account_info(),
        ctx.accounts.vault_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        amount,
        ctx.accounts.vault_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[auth_bump]]],
    )
}
