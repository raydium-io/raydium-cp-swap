use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseCreatorFeeShare<'info> {
    #[account(
        mut,
        constraint = (owner.key() == crate::admin::ID || owner.key() == crate::create_creator_fee_share_owner::ID) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// CHECK: the pool creator the custom share rate applies to
    pub creator: UncheckedAccount<'info>,

    /// The amm config the custom share rate applies to
    pub amm_config: Account<'info, AmmConfig>,

    /// The custom share account to close, the creator fee falls back to the rate
    /// configured on `amm_config` afterwards
    #[account(
        mut,
        seeds = [
            CREATOR_FEE_SHARE_SEED.as_bytes(),
            creator.key().as_ref(),
            amm_config.key().as_ref(),
        ],
        bump,
        close = owner
    )]
    pub creator_fee_share: Account<'info, CreatorFeeShare>,

    pub system_program: Program<'info, System>,
}

pub fn close_creator_fee_share(_ctx: Context<CloseCreatorFeeShare>) -> Result<()> {
    Ok(())
}
