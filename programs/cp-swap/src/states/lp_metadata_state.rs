use anchor_lang::prelude::*;

pub const LP_METADATA_STATE_SEED: &str = "lp_metadata_state";

/// Tracks who created the LP token metadata for a pool.
///
/// First-come-first-serve: the PDA is derived from the pool state, so only the
/// first caller to initialize it wins, and only that creator may update the
/// metadata afterwards.
#[account]
#[derive(Default, Debug)]
pub struct LpMetadataState {
    /// Bump to identify PDA
    pub bump: u8,
    /// The wallet that created the LP metadata, the only one allowed to update it
    pub creator: Pubkey,
    pub padding: [u64; 8],
}

impl LpMetadataState {
    pub const LEN: usize = 8 + 1 + 32 + 64;
}
