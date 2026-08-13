use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

/// Escape hatch for the permissionless "goldrush" LP metadata claim.
///
/// Claiming LP metadata is intentionally first-come-first-serve, but that is a
/// liability risk: a griefer can front-run the pool creator and lock the LP
/// token's name/symbol/uri, or the original creator can lose their key. Either
/// the pool creator or the protocol admin may therefore reclaim the update
/// authority at any time, becoming the new `creator`.
#[derive(Accounts)]
pub struct ReclaimLpMetadata<'info> {
    /// The pool creator or the protocol admin, reclaiming update authority.
    #[account(
        constraint = reclaimer.key() == pool_state.load()?.pool_creator
            || reclaimer.key() == amm_config.protocol_owner
            || reclaimer.key() == crate::admin::ID
            @ ErrorCode::InvalidOwner
    )]
    pub reclaimer: Signer<'info>,

    /// Pool state, used to derive the metadata state PDA and the pool creator.
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Amm config account storing the protocol owner.
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Account<'info, AmmConfig>,

    /// Metadata state account storing the creator.
    #[account(
        mut,
        seeds = [
            LP_METADATA_STATE_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
    )]
    pub lp_metadata_state: Account<'info, LpMetadataState>,
}

pub fn reclaim_lp_metadata(ctx: Context<ReclaimLpMetadata>) -> Result<()> {
    ctx.accounts.lp_metadata_state.creator = ctx.accounts.reclaimer.key();
    Ok(())
}
