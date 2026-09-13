use crate::curve::fees::Fees;
use crate::curve::stable;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

/// Swap between two members of a collection pool, inside the pool. Priced by a StableSwap over
/// all members' rate-normalised reserves (exactly the collection rate at balance), fee is
/// `amm_config.trade_fee_rate / collection.rebalance_fee_divisor` on input, split like any swap.
#[derive(Accounts)]
pub struct IntraSwap<'info> {
    pub payer: Signer<'info>,

    /// CHECK: pool vault authority
    #[account(seeds = [crate::AUTH_SEED.as_bytes()], bump)]
    pub authority: UncheckedAccount<'info>,

    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [POOL_MEMBERS_SEED.as_bytes(), pool_state.key().as_ref()],
        bump = pool_members.bump,
    )]
    pub pool_members: Box<Account<'info, PoolMembers>>,

    #[account(address = pool_members.collection @ ErrorCode::InvalidCollectionMember)]
    pub collection: Box<Account<'info, TokenCollection>>,

    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub input_token_program: Interface<'info, TokenInterface>,
    pub output_token_program: Interface<'info, TokenInterface>,
    #[account(address = input_vault.mint)]
    pub input_token_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(address = output_vault.mint)]
    pub output_token_mint: Box<InterfaceAccount<'info, Mint>>,
    // remaining accounts, for k in 0..pool_members.n: [member vault k, CollectionMember k]
}

#[event]
pub struct IntraSwapEvent {
    pub pool_id: Pubkey,
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub input_amount: u64,
    pub output_amount: u64,
    pub trade_fee: u64,
}

pub fn intra_swap<'info>(
    ctx: Context<'info, IntraSwap<'info>>,
    from_index: u8,
    to_index: u8,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    let block_timestamp = Clock::get()?.unix_timestamp as u64;
    let pm = &ctx.accounts.pool_members;
    let n = pm.n as usize;
    let (i, j) = (from_index as usize, to_index as usize);
    require!(i < n && j < n && i != j, ErrorCode::InvalidPoolMember);
    require_keys_eq!(ctx.accounts.input_vault.key(), pm.members[i].vault, ErrorCode::InvalidPoolMember);
    require_keys_eq!(ctx.accounts.output_vault.key(), pm.members[j].vault, ErrorCode::InvalidPoolMember);
    require!(ctx.remaining_accounts.len() >= 2 * n, ErrorCode::InvalidPoolMember);
    let pool_key = ctx.accounts.pool_state.key();
    let pool = ctx.accounts.pool_state.load()?;
    require!(pool.get_status_by_bit(PoolStatusBitIndex::Swap) && block_timestamp >= pool.open_time, ErrorCode::NotApproved);

    // Reserves (fees excluded) and rates for every member.
    let mut reserves = [0u128; MAX_POOL_MEMBERS];
    let mut rates = [0u64; MAX_POOL_MEMBERS];
    for k in 0..n {
        let vault_info = &ctx.remaining_accounts[2 * k];
        let member_info = &ctx.remaining_accounts[2 * k + 1];
        require_keys_eq!(vault_info.key(), pm.members[k].vault, ErrorCode::InvalidPoolMember);
        let vault = InterfaceAccount::<TokenAccount>::try_from(vault_info)?;
        let member = Account::<CollectionMember>::try_from(member_info)?;
        require!(member.collection == pm.collection && member.mint == pm.members[k].mint, ErrorCode::InvalidCollectionMember);
        let owed = if k == 0 {
            if pm.base_is_token_0 {
                pool.protocol_fees_token_0 + pool.fund_fees_token_0 + pool.creator_fees_token_0
            } else {
                pool.protocol_fees_token_1 + pool.fund_fees_token_1 + pool.creator_fees_token_1
            }
        } else {
            pm.members[k].protocol_fees_owed + pm.members[k].fund_fees_owed
        };
        reserves[k] = u128::from(vault.amount.checked_sub(owed).ok_or(ErrorCode::InsufficientVault)?);
        rates[k] = member.rate;
    }
    drop(pool);
    let mut xp = vec![0u128; n];
    for k in 0..n {
        xp[k] = reserves[k].checked_mul(u128::from(rates[k])).ok_or(ErrorCode::MathOverflow)?;
    }

    let transfer_fee = get_transfer_fee(&ctx.accounts.input_token_mint.to_account_info(), amount_in)?;
    let actual_in = amount_in.saturating_sub(transfer_fee);
    require_gt!(actual_in, 0);
    // Never below 1 ppm.
    let fee_rate = (ctx.accounts.amm_config.trade_fee_rate / u64::from(ctx.accounts.collection.rebalance_fee_divisor)).max(1);
    let trade_fee = Fees::trading_fee(u128::from(actual_in), fee_rate).ok_or(ErrorCode::MathOverflow)?;
    let protocol_fee = Fees::protocol_fee(trade_fee, ctx.accounts.amm_config.protocol_fee_rate).ok_or(ErrorCode::MathOverflow)?;
    let fund_fee = Fees::fund_fee(trade_fee, ctx.accounts.amm_config.fund_fee_rate).ok_or(ErrorCode::MathOverflow)?;
    let net_in = u128::from(actual_in).checked_sub(trade_fee).ok_or(ErrorCode::MathOverflow)?;
    let dx = net_in.checked_mul(u128::from(rates[i])).ok_or(ErrorCode::MathOverflow)?;
    let dy = stable::swap_out(&xp, i, j, dx, pm.amp).ok_or(ErrorCode::StableCurveConvergence)?;
    let amount_out = u64::try_from(dy / u128::from(rates[j])).map_err(|_| ErrorCode::MathOverflow)?;
    require_gt!(amount_out, 0, ErrorCode::ZeroTradingTokens);
    require!(u128::from(amount_out) < reserves[j], ErrorCode::InsufficientMemberLiquidity);
    let out_transfer_fee = get_transfer_fee(&ctx.accounts.output_token_mint.to_account_info(), amount_out)?;
    require_gte!(amount_out.checked_sub(out_transfer_fee).ok_or(ErrorCode::MathOverflow)?, minimum_amount_out, ErrorCode::ExceededSlippage);

    // Fee accounting: base member on PoolState, others on PoolMembers.
    let (protocol_fee, fund_fee) = (u64::try_from(protocol_fee).unwrap(), u64::try_from(fund_fee).unwrap());
    if i == 0 {
        let mut pool = ctx.accounts.pool_state.load_mut()?;
        if pm.base_is_token_0 {
            pool.protocol_fees_token_0 = pool.protocol_fees_token_0.checked_add(protocol_fee).unwrap();
            pool.fund_fees_token_0 = pool.fund_fees_token_0.checked_add(fund_fee).unwrap();
        } else {
            pool.protocol_fees_token_1 = pool.protocol_fees_token_1.checked_add(protocol_fee).unwrap();
            pool.fund_fees_token_1 = pool.fund_fees_token_1.checked_add(fund_fee).unwrap();
        }
    } else {
        let pm = &mut ctx.accounts.pool_members;
        pm.members[i].protocol_fees_owed = pm.members[i].protocol_fees_owed.checked_add(protocol_fee).unwrap();
        pm.members[i].fund_fees_owed = pm.members[i].fund_fees_owed.checked_add(fund_fee).unwrap();
    }

    transfer_from_user_to_pool_vault(
        ctx.accounts.payer.to_account_info(),
        ctx.accounts.input_token_account.to_account_info(),
        ctx.accounts.input_vault.to_account_info(),
        ctx.accounts.input_token_mint.to_account_info(),
        ctx.accounts.input_token_program.to_account_info(),
        amount_in,
        ctx.accounts.input_token_mint.decimals,
    )?;
    let auth_bump = ctx.accounts.pool_state.load()?.auth_bump;
    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.output_vault.to_account_info(),
        ctx.accounts.output_token_account.to_account_info(),
        ctx.accounts.output_token_mint.to_account_info(),
        ctx.accounts.output_token_program.to_account_info(),
        amount_out,
        ctx.accounts.output_token_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[auth_bump]]],
    )?;
    emit!(IntraSwapEvent {
        pool_id: pool_key,
        input_mint: ctx.accounts.input_token_mint.key(),
        output_mint: ctx.accounts.output_token_mint.key(),
        input_amount: actual_in,
        output_amount: amount_out,
        trade_fee: u64::try_from(trade_fee).unwrap(),
    });
    Ok(())
}
