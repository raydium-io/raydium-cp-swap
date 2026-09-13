use crate::error::ErrorCode;
use crate::instructions::swap_base_input::*;
use crate::states::*;
use anchor_lang::prelude::*;

/// `swap_base_input` for a pool whose two mints are members of the same token collection.
/// The LP fee is `amm_config.trade_fee_rate / collection.rebalance_fee_divisor` and the swap is
/// only accepted if it strictly reduces the pool's imbalance against the members' rates.
#[derive(Accounts)]
pub struct RebalanceSwap<'info> {
    pub swap: Swap<'info>,

    #[account(
        seeds = [
            TOKEN_COLLECTION_SEED.as_bytes(),
            collection.authority.as_ref(),
            &collection.index.to_le_bytes()
        ],
        bump = collection.bump,
    )]
    pub collection: Box<Account<'info, TokenCollection>>,

    #[account(
        seeds = [
            COLLECTION_MEMBER_SEED.as_bytes(),
            collection.key().as_ref(),
            swap.input_token_mint.key().as_ref()
        ],
        bump = input_member.bump,
        constraint = input_member.collection == collection.key() @ ErrorCode::InvalidCollectionMember,
    )]
    pub input_member: Box<Account<'info, CollectionMember>>,

    #[account(
        seeds = [
            COLLECTION_MEMBER_SEED.as_bytes(),
            collection.key().as_ref(),
            swap.output_token_mint.key().as_ref()
        ],
        bump = output_member.bump,
        constraint = output_member.collection == collection.key() @ ErrorCode::InvalidCollectionMember,
    )]
    pub output_member: Box<Account<'info, CollectionMember>>,
}

pub fn rebalance_swap_base_input(
    ctx: Context<RebalanceSwap>,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    // Never below 1 ppm: a zero fee rate would make the creator-fee split divide by zero.
    let fee_rate = ctx
        .accounts
        .swap
        .amm_config
        .trade_fee_rate
        .checked_div(u64::from(ctx.accounts.collection.rebalance_fee_divisor))
        .ok_or(ErrorCode::InvalidInput)?
        .max(1);
    let guard = RebalanceGuard {
        input_rate: ctx.accounts.input_member.rate,
        output_rate: ctx.accounts.output_member.rate,
    };
    swap_base_input_with_fee(
        &mut ctx.accounts.swap,
        amount_in,
        minimum_amount_out,
        fee_rate,
        Some(guard),
    )
}
