use crate::curve::CurveCalculator;
use crate::curve::RoundDirection;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[event_cpi]
#[derive(Accounts)]
pub struct DepositV2<'info> {
    /// Owner of position, payer for add liquidity
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
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        mut,
        address = position.pool_id
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The payer's token account for token_0
    #[account(
        mut,
        token::mint = vault_0_mint,
        token::authority = position_nft_owner
    )]
    pub token_0_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The payer's token account for token_1
    #[account(
        mut,
        token::mint = vault_1_mint,
        token::authority = position_nft_owner
    )]
    pub token_1_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        token::mint = vault_0_mint,
        address = pool_state.load()?.token_0_vault
    )]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        token::mint = vault_1_mint,
        address = pool_state.load()?.token_1_vault
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

pub fn deposit_v2(
    ctx: Context<DepositV2>,
    lp_amount: u64,
    maximum_token_0_amount: u64,
    maximum_token_1_amount: u64,
) -> Result<()> {
    require_gt!(lp_amount, 0);

    let pool_id = ctx.accounts.pool_state.key();
    let pool_state = &mut ctx.accounts.pool_state.load_mut()?;
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::Deposit) {
        return err!(ErrorCode::NotApproved);
    }
    ctx.accounts.position.update_fee(
        pool_state.fees_token_0_per_lp,
        pool_state.fees_token_1_per_lp,
    )?;
    let (total_token_0_amount, total_token_1_amount) = pool_state.vault_amount_without_fee(
        ctx.accounts.token_0_vault.amount,
        ctx.accounts.token_1_vault.amount,
    )?;
    let results = CurveCalculator::lp_tokens_to_trading_tokens(
        u128::from(lp_amount),
        u128::from(pool_state.lp_supply),
        u128::from(total_token_0_amount),
        u128::from(total_token_1_amount),
        RoundDirection::Ceiling,
    )
    .ok_or(ErrorCode::ZeroTradingTokens)?;
    if results.token_0_amount == 0 || results.token_1_amount == 0 {
        return err!(ErrorCode::ZeroTradingTokens);
    }
    let token_0_amount = u64::try_from(results.token_0_amount).unwrap();
    let (transfer_token_0_amount, transfer_token_0_fee) = {
        let transfer_fee =
            get_transfer_inverse_fee(&ctx.accounts.vault_0_mint.to_account_info(), token_0_amount)?;
        (
            token_0_amount.checked_add(transfer_fee).unwrap(),
            transfer_fee,
        )
    };

    let token_1_amount = u64::try_from(results.token_1_amount).unwrap();
    let (transfer_token_1_amount, transfer_token_1_fee) = {
        let transfer_fee =
            get_transfer_inverse_fee(&ctx.accounts.vault_1_mint.to_account_info(), token_1_amount)?;
        (
            token_1_amount.checked_add(transfer_fee).unwrap(),
            transfer_fee,
        )
    };

    #[cfg(feature = "enable-log")]
    msg!(
        "results.token_0_amount;{}, results.token_1_amount:{},transfer_token_0_amount:{},transfer_token_0_fee:{},
            transfer_token_1_amount:{},transfer_token_1_fee:{}",
        results.token_0_amount,
        results.token_1_amount,
        transfer_token_0_amount,
        transfer_token_0_fee,
        transfer_token_1_amount,
        transfer_token_1_fee
    );

    emit_cpi!(LpChangeEvent {
        pool_id,
        lp_amount_before: pool_state.lp_supply,
        token_0_vault_before: total_token_0_amount,
        token_1_vault_before: total_token_1_amount,
        token_0_amount,
        token_1_amount,
        token_0_transfer_fee: transfer_token_0_fee,
        token_1_transfer_fee: transfer_token_1_fee,
        change_type: 0
    });

    if transfer_token_0_amount > maximum_token_0_amount
        || transfer_token_1_amount > maximum_token_1_amount
    {
        return Err(ErrorCode::ExceededSlippage.into());
    }

    transfer_from_user_to_pool_vault(
        ctx.accounts.position_nft_owner.to_account_info(),
        ctx.accounts.token_0_account.to_account_info(),
        ctx.accounts.token_0_vault.to_account_info(),
        ctx.accounts.vault_0_mint.to_account_info(),
        ctx.accounts.token_0_program.to_account_info(),
        transfer_token_0_amount,
        ctx.accounts.vault_0_mint.decimals,
    )?;

    transfer_from_user_to_pool_vault(
        ctx.accounts.position_nft_owner.to_account_info(),
        ctx.accounts.token_1_account.to_account_info(),
        ctx.accounts.token_1_vault.to_account_info(),
        ctx.accounts.vault_1_mint.to_account_info(),
        ctx.accounts.token_1_program.to_account_info(),
        transfer_token_1_amount,
        ctx.accounts.vault_1_mint.decimals,
    )?;

    ctx.accounts.position.update_lp_amount(lp_amount, true)?;
    pool_state.update_lp_supply(lp_amount, true, Clock::get()?.epoch)?;

    Ok(())
}
