use crate::curve::CurveCalculator;
use crate::curve::RoundDirection;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

#[derive(Accounts)]
pub struct Deposit<'info> {
    /// Pays to mint the position
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Owner lp tokan account
    #[account(mut,  token::authority = owner)]
    pub owner_lp_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The payer's token account for token_0
    #[account(
        mut,
        token::mint = token_0_vault.mint,
        token::authority = owner
    )]
    pub token_0_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The payer's token account for token_1
    #[account(
        mut,
        token::mint = token_1_vault.mint,
        token::authority = owner
    )]
    pub token_1_account: Box<InterfaceAccount<'info, TokenAccount>>,

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

    /// token Program
    pub token_program: Program<'info, Token>,

    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,

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

    /// Lp token mint
    #[account(address = pool_state.load()?.lp_mint @ ErrorCode::IncorrectLpMint)]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,
}

pub fn deposit(
    ctx: Context<Deposit>,
    lp_token_amount: u64,
    maximum_token_0_amount: u64,
    maximum_token_1_amount: u64,
) -> Result<()> {
    require_gt!(ctx.accounts.lp_mint.supply, 0);

    let pool_state = ctx.accounts.pool_state.load()?;
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::Deposit) {
        return err!(ErrorCode::NotApproved);
    }
    let (total_token_0_amount, total_token_1_amount) = pool_state.vault_amount_without_fee(
        ctx.accounts.token_0_vault.amount,
        ctx.accounts.token_1_vault.amount,
    );
    let results = CurveCalculator::lp_tokens_to_trading_tokens(
        u128::from(lp_token_amount),
        u128::from(ctx.accounts.lp_mint.supply),
        u128::from(total_token_0_amount),
        u128::from(total_token_1_amount),
        RoundDirection::Ceiling,
    )
    .ok_or(ErrorCode::ZeroTradingTokens)?;

    let token_0_amount = u64::try_from(results.token_0_amount).unwrap();
    let transfer_token_0_amount = token_0_amount
        .checked_add(get_transfer_inverse_fee(
            &ctx.accounts.vault_0_mint,
            token_0_amount,
        )?)
        .unwrap();

    if transfer_token_0_amount > maximum_token_0_amount {
        return Err(ErrorCode::ExceededSlippage.into());
    }

    let token_1_amount = u64::try_from(results.token_1_amount).unwrap();
    let transfer_token_1_amount = token_0_amount
        .checked_add(get_transfer_inverse_fee(
            &ctx.accounts.vault_1_mint,
            token_1_amount,
        )?)
        .unwrap();
    if transfer_token_1_amount > maximum_token_1_amount {
        return Err(ErrorCode::ExceededSlippage.into());
    }

    transfer_from_user_to_pool_vault(
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_0_account.to_account_info(),
        ctx.accounts.token_0_vault.to_account_info(),
        ctx.accounts.vault_0_mint.to_account_info(),
        if ctx.accounts.vault_0_mint.to_account_info().owner == ctx.accounts.token_program.key {
            ctx.accounts.token_program.to_account_info()
        } else {
            ctx.accounts.token_program_2022.to_account_info()
        },
        transfer_token_0_amount,
        ctx.accounts.vault_0_mint.decimals,
    )?;

    transfer_from_user_to_pool_vault(
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_1_account.to_account_info(),
        ctx.accounts.token_1_vault.to_account_info(),
        ctx.accounts.vault_1_mint.to_account_info(),
        if ctx.accounts.vault_1_mint.to_account_info().owner == ctx.accounts.token_program.key {
            ctx.accounts.token_program.to_account_info()
        } else {
            ctx.accounts.token_program_2022.to_account_info()
        },
        transfer_token_1_amount,
        ctx.accounts.vault_1_mint.decimals,
    )?;

    token_mint_to(
        &ctx.accounts.pool_state,
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.lp_mint.to_account_info(),
        ctx.accounts.owner_lp_token.to_account_info(),
        lp_token_amount,
    )
}
