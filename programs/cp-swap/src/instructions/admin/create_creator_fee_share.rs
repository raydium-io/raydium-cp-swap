use crate::curve::fees::FEE_RATE_DENOMINATOR_VALUE;
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use std::ops::DerefMut;

pub mod create_creator_fee_share_owner {
    use super::{pubkey, Pubkey};
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("DRayDe7AGe6nFg6egr7D42ECyDf15GV52WFwZAbfUo1R");
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("RayFfEhJHWToYRsr8sjVZZkdddBeYxyJazDitpdE7zJ");
}

#[derive(Accounts)]
pub struct CreateCreatorFeeShare<'info> {
    #[account(
        mut,
        constraint = (owner.key() == crate::admin::ID || owner.key() == crate::create_creator_fee_share_owner::ID) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// CHECK: the pool creator the custom share rate applies to
    pub creator: UncheckedAccount<'info>,

    /// The amm config the custom share rate applies to
    pub amm_config: Account<'info, AmmConfig>,

    /// Stores the custom share of the creator fee retained by the protocol
    #[account(
        init,
        seeds = [
            CREATOR_FEE_SHARE_SEED.as_bytes(),
            creator.key().as_ref(),
            amm_config.key().as_ref(),
        ],
        bump,
        payer = owner,
        space = CreatorFeeShare::LEN
    )]
    pub creator_fee_share: Account<'info, CreatorFeeShare>,

    pub system_program: Program<'info, System>,
}

pub fn create_creator_fee_share(
    ctx: Context<CreateCreatorFeeShare>,
    share_rate: u64,
) -> Result<()> {
    require_gte!(
        FEE_RATE_DENOMINATOR_VALUE,
        share_rate,
        ErrorCode::InvalidInput
    );
    let bump = ctx.bumps.creator_fee_share;
    let creator = ctx.accounts.creator.key();
    let amm_config = ctx.accounts.amm_config.key();
    let creator_fee_share = ctx.accounts.creator_fee_share.deref_mut();
    creator_fee_share.bump = bump;
    creator_fee_share.creator = creator;
    creator_fee_share.amm_config = amm_config;
    creator_fee_share.share_rate = share_rate;
    Ok(())
}
