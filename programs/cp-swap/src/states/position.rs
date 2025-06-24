use anchor_lang::prelude::*;

use crate::{error::ErrorCode, states::Q32, utils::DownCast};

pub const POSITION_SEED: &str = "position";

/// Holds the current owner of the factory
#[account]
#[derive(Default, Debug)]
pub struct Position {
    // account update recent epoch
    pub recent_epoch: u64,
    /// Bump to identify PDA
    pub bump: [u8; 1],
    /// The amount of liquidity owned by this position
    pub lp_amount: u64,
    /// The fees owed to the position owner in token_0
    pub fees_owed_token_0: u64,
    /// The fees owed to the position owner in token_1
    pub fees_owed_token_1: u64,
    /// The value of lp_fees_token_0_per_lp in the pool when the position fee was last updated.
    pub fees_token_0_per_lp_last: u128,
    /// The value of lp_fees_token_1_per_lp in the pool when the position fee was last updated.
    pub fees_token_1_per_lp_last: u128,
    /// Mint address of the tokenized position
    pub nft_mint: Pubkey,
    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,
    // Unused bytes for future upgrades.
    pub padding: [u64; 8],
}

impl Position {
    pub const LEN: usize = 8 + 8 + 1 + 8 * 3 + 16 * 2 + 32 * 2 + 8 * 8;

    pub fn seeds(&self) -> [&[u8]; 3] {
        [
            &POSITION_SEED.as_bytes(),
            self.nft_mint.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn initialize(
        &mut self,
        bump: u8,
        lp_amount: u64,
        recent_epoch: u64,
        fees_token_0_per_lp: u128,
        fees_token_1_per_lp: u128,
        nft_mint: Pubkey,
        pool_id: Pubkey,
    ) -> Result<()> {
        self.bump = [bump];
        self.recent_epoch = recent_epoch;
        self.lp_amount = lp_amount;
        self.fees_owed_token_0 = 0;
        self.fees_owed_token_1 = 0;
        self.fees_token_0_per_lp_last = fees_token_0_per_lp;
        self.fees_token_1_per_lp_last = fees_token_1_per_lp;
        self.nft_mint = nft_mint;
        self.pool_id = pool_id;
        Ok(())
    }

    pub fn update_lp_amount(&mut self, liquidity_delta: u64, add: bool) -> Result<()> {
        if add {
            self.lp_amount = self
                .lp_amount
                .checked_add(liquidity_delta)
                .ok_or(ErrorCode::MathOverflow)?;
        } else {
            self.lp_amount = self
                .lp_amount
                .checked_sub(liquidity_delta)
                .ok_or(ErrorCode::MathOverflow)?;
        }
        Ok(())
    }

    pub fn update_fee(
        &mut self,
        fees_token_0_per_lp: u128,
        fees_token_1_per_lp: u128,
    ) -> Result<()> {
        if self.lp_amount > 0 {
            let fee_delta_0 = fees_token_0_per_lp
                .checked_sub(self.fees_token_0_per_lp_last)
                .ok_or(ErrorCode::MathOverflow)?
                * u128::from(self.lp_amount)
                / Q32;
            self.fees_owed_token_0 = self
                .fees_owed_token_0
                .checked_add(fee_delta_0.to_u64().ok_or(ErrorCode::MathOverflow)?)
                .ok_or(ErrorCode::MathOverflow)?;

            let fee_delta_1 = fees_token_1_per_lp
                .checked_sub(self.fees_token_1_per_lp_last)
                .ok_or(ErrorCode::MathOverflow)?
                * u128::from(self.lp_amount)
                / Q32;
            self.fees_owed_token_1 = self
                .fees_owed_token_1
                .checked_add(fee_delta_1.to_u64().ok_or(ErrorCode::MathOverflow)?)
                .ok_or(ErrorCode::MathOverflow)?;
        }
        self.fees_token_0_per_lp_last = fees_token_0_per_lp;
        self.fees_token_1_per_lp_last = fees_token_1_per_lp;
        Ok(())
    }
}
