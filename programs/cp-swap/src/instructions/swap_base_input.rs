use crate::curve::calculator::CurveCalculator;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Swap<'info> {
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// CHECK: pool vault and lp mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(
        mut,
        constraint = input_vault.key() == pool_state.load()?.token_0_vault || input_vault.key() == pool_state.load()?.token_1_vault
    )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(
        mut,
        constraint = output_vault.key() == pool_state.load()?.token_0_vault || output_vault.key() == pool_state.load()?.token_1_vault
    )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL program for input token transfers
    pub input_token_program: Interface<'info, TokenInterface>,

    /// SPL program for output token transfers
    pub output_token_program: Interface<'info, TokenInterface>,

    /// The mint of input token
    #[account(
        address = input_vault.mint
    )]
    pub input_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output token
    #[account(
        address = output_vault.mint
    )]
    pub output_token_mint: Box<InterfaceAccount<'info, Mint>>,
    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,
}

pub fn swap_base_input(ctx: Context<Swap>, amount_in: u64, minimum_amount_out: u64) -> Result<()> {
    let trade_fee_rate = ctx.accounts.amm_config.trade_fee_rate;
    swap_base_input_with_fee(ctx.accounts, amount_in, minimum_amount_out, trade_fee_rate, None)
}

/// Optional guard for collection rebalance swaps: rates (1e9 == 1.0) valuing one whole input /
/// output token in the collection numeraire. The swap must strictly reduce
/// `|value(vault_0) - value(vault_1)|`.
pub struct RebalanceGuard {
    pub input_rate: u64,
    pub output_rate: u64,
}

/// Shared body of `swap_base_input` and `rebalance_swap_base_input`; `trade_fee_rate` is the
/// effective LP fee rate for this swap (the config rate, or a discounted one).
pub fn swap_base_input_with_fee<'info>(
    accounts: &mut Swap<'info>,
    amount_in: u64,
    minimum_amount_out: u64,
    trade_fee_rate: u64,
    rebalance: Option<RebalanceGuard>,
) -> Result<()> {
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    let pool_id = accounts.pool_state.key();
    let pool_state = &mut accounts.pool_state.load_mut()?;
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::Swap)
        || block_timestamp < pool_state.open_time
    {
        return err!(ErrorCode::NotApproved);
    }

    let transfer_fee =
        get_transfer_fee(&accounts.input_token_mint.to_account_info(), amount_in)?;
    // Take transfer fees into account for actual amount transferred in
    let actual_amount_in = amount_in.saturating_sub(transfer_fee);
    require_gt!(actual_amount_in, 0);

    let SwapParams {
        trade_direction,
        total_input_token_amount,
        total_output_token_amount,
        token_0_price_x64,
        token_1_price_x64,
        is_creator_fee_on_input,
    } = pool_state.get_swap_params(
        accounts.input_vault.key(),
        accounts.output_vault.key(),
        accounts.input_vault.amount,
        accounts.output_vault.amount,
    )?;
    let constant_before = u128::from(total_input_token_amount)
        .checked_mul(u128::from(total_output_token_amount))
        .unwrap();

    let creator_fee_rate =
        pool_state.adjust_creator_fee_rate(accounts.amm_config.creator_fee_rate);
    let result = CurveCalculator::swap_base_input(
        u128::from(actual_amount_in),
        u128::from(total_input_token_amount),
        u128::from(total_output_token_amount),
        trade_fee_rate,
        creator_fee_rate,
        accounts.amm_config.protocol_fee_rate,
        accounts.amm_config.fund_fee_rate,
        is_creator_fee_on_input,
    )
    .ok_or(ErrorCode::ZeroTradingTokens)?;

    let constant_after = u128::from(result.new_input_vault_amount)
        .checked_mul(u128::from(result.new_output_vault_amount))
        .unwrap();
    #[cfg(feature = "enable-log")]
    msg!(
        "input_amount:{}, output_amount:{}, trade_fee:{}, input_transfer_fee:{}, constant_before:{},constant_after:{}, is_creator_fee_on_input:{}, creator_fee:{}",
        result.input_amount,
        result.output_amount,
        result.trade_fee,
        transfer_fee,
        constant_before,
        constant_after,
        is_creator_fee_on_input,
        result.creator_fee,
    );
    require_eq!(
        u64::try_from(result.input_amount).unwrap(),
        actual_amount_in
    );
    let (input_transfer_amount, input_transfer_fee) = (amount_in, transfer_fee);
    let (output_transfer_amount, output_transfer_fee) = {
        let amount_out = u64::try_from(result.output_amount).unwrap();
        let transfer_fee = get_transfer_fee(
            &accounts.output_token_mint.to_account_info(),
            amount_out,
        )?;
        let amount_received = amount_out.checked_sub(transfer_fee).unwrap();
        require_gt!(amount_received, 0);
        require_gte!(
            amount_received,
            minimum_amount_out,
            ErrorCode::ExceededSlippage
        );
        (amount_out, transfer_fee)
    };

    pool_state.update_fees(
        u64::try_from(result.protocol_fee).unwrap(),
        u64::try_from(result.fund_fee).unwrap(),
        u64::try_from(result.creator_fee).unwrap(),
        trade_direction,
    )?;

    emit!(SwapEvent {
        pool_id,
        input_vault_before: total_input_token_amount,
        output_vault_before: total_output_token_amount,
        input_amount: u64::try_from(result.input_amount).unwrap(),
        output_amount: u64::try_from(result.output_amount).unwrap(),
        input_transfer_fee,
        output_transfer_fee,
        base_input: true,
        input_mint: accounts.input_token_mint.key(),
        output_mint: accounts.output_token_mint.key(),
        trade_fee: u64::try_from(result.trade_fee).unwrap(),
        creator_fee: u64::try_from(result.creator_fee).unwrap(),
        creator_fee_on_input: is_creator_fee_on_input,
    });
    require_gte!(constant_after, constant_before);
    if let Some(guard) = rebalance {
        let value = |amount: u64, decimals: u8, rate: u64| -> Option<u128> {
            u128::from(amount)
                .checked_mul(10u128.checked_pow(18u32.checked_sub(u32::from(decimals))?)?)?
                .checked_mul(u128::from(rate))?
                .checked_div(u128::from(crate::states::RATE_ONE))
        };
        let (in_dec, out_dec) = (
            accounts.input_token_mint.decimals,
            accounts.output_token_mint.decimals,
        );
        let imbalance = |input_vault: u128, output_vault: u128| -> Option<u128> {
            let a = value(u64::try_from(input_vault).ok()?, in_dec, guard.input_rate)?;
            let b = value(u64::try_from(output_vault).ok()?, out_dec, guard.output_rate)?;
            Some(a.abs_diff(b))
        };
        let before = imbalance(
            u128::from(total_input_token_amount),
            u128::from(total_output_token_amount),
        )
        .ok_or(ErrorCode::MathOverflow)?;
        let after = imbalance(result.new_input_vault_amount, result.new_output_vault_amount)
            .ok_or(ErrorCode::MathOverflow)?;
        require_gt!(before, after, ErrorCode::NotRebalancing);
    }

    transfer_from_user_to_pool_vault(
        accounts.payer.to_account_info(),
        accounts.input_token_account.to_account_info(),
        accounts.input_vault.to_account_info(),
        accounts.input_token_mint.to_account_info(),
        accounts.input_token_program.to_account_info(),
        input_transfer_amount,
        accounts.input_token_mint.decimals,
    )?;

    transfer_from_pool_vault_to_user(
        accounts.authority.to_account_info(),
        accounts.output_vault.to_account_info(),
        accounts.output_token_account.to_account_info(),
        accounts.output_token_mint.to_account_info(),
        accounts.output_token_program.to_account_info(),
        output_transfer_amount,
        accounts.output_token_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[pool_state.auth_bump]]],
    )?;

    // update the previous price to the observation
    accounts.observation_state.load_mut()?.update(
        oracle::block_timestamp(),
        token_0_price_x64,
        token_1_price_x64,
    )?;
    pool_state.recent_epoch = Clock::get()?.epoch;

    Ok(())
}
