use crate::error::ErrorCode;
/// Oracle provides price data useful for a wide variety of system designs
///
use anchor_lang::prelude::*;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};
/// Seed to derive account address and signature
pub const OBSERVATION_SEED: &str = "observation";
// Number of ObservationState element
pub const OBSERVATION_NUM: usize = 100;
pub const OBSERVATION_UPDATE_DURATION_DEFAULT: u64 = 15;

/// The element of observations in ObservationState
#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct Observation {
    /// The block timestamp of the observation
    pub block_timestamp: u64,
    /// the cumulative of token0 price during the duration time, Q32.32, the remaining 64 bit for overflow
    pub cumulative_token_0_price_x32: u128,
    /// the cumulative of token1 price during the duration time, Q32.32, the remaining 64 bit for overflow
    pub cumulative_token_1_price_x32: u128,
}
impl Observation {
    pub const LEN: usize = 8 + 16 + 16;
}

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ObservationState {
    /// Whether the ObservationState is initialized
    pub initialized: bool,
    /// the most-recently updated index of the observations array
    pub observation_index: u16,
    pub pool_id: Pubkey,
    /// observation array
    pub observations: [Observation; OBSERVATION_NUM],
    /// the last update timestamp
    pub last_update_timestamp: u64,
    /// padding for feature update
    pub padding: [u64; 3],
}

impl Default for ObservationState {
    #[inline]
    fn default() -> ObservationState {
        ObservationState {
            initialized: false,
            observation_index: 0,
            pool_id: Pubkey::default(),
            observations: [Observation::default(); OBSERVATION_NUM],
            last_update_timestamp: 0,
            padding: [0u64; 3],
        }
    }
}

impl ObservationState {
    pub const LEN: usize = 8 + 1 + 2 + 32 + (Observation::LEN * OBSERVATION_NUM) + 8 * 4;

    // Writes an oracle observation to the account, returning the next observation_index.
    /// Writable at most once per second. Index represents the most recently written element.
    /// If the index is at the end of the allowable array length (100 - 1), the next index will turn to 0.
    ///
    /// # Arguments
    ///
    /// * `self` - The ObservationState account to write in
    /// * `block_timestamp` - The current timestamp of to update
    /// * `token_0_price_x32` - The token_0_price_x32 at the time of the new observation
    /// * `token_1_price_x32` - The token_1_price_x32 at the time of the new observation
    /// * `observation_index` - The last update index of element in the oracle array
    ///
    /// # Return
    /// * `next_observation_index` - The new index of element to update in the oracle array
    ///
    pub fn update(
        &mut self,
        block_timestamp: u64,
        token_0_price_x32: u128,
        token_1_price_x32: u128,
    ) -> Result<()> {
        let observation_index = self.observation_index;
        if !self.initialized {
            // skip the pool init price
            self.initialized = true;
            self.observations[observation_index as usize].block_timestamp = block_timestamp;
            self.observations[observation_index as usize].cumulative_token_0_price_x32 = 0;
            self.observations[observation_index as usize].cumulative_token_1_price_x32 = 0;
            self.last_update_timestamp = block_timestamp;
            return Ok(());
        }
        let last_observation = self.observations[observation_index as usize];
        let next_observation_index = if observation_index as usize == OBSERVATION_NUM - 1 {
            0
        } else {
            observation_index + 1
        };
        // Ensure last_update_timestamp is set for legacy accounts
        if self.last_update_timestamp == 0 {
            self.last_update_timestamp = last_observation.block_timestamp;
        }
        let time_since_last_observation =
            block_timestamp.saturating_sub(last_observation.block_timestamp);
        // Accumulate using last known price over the elapsed time
        let time_since_last_update = block_timestamp.saturating_sub(self.last_update_timestamp);
        if time_since_last_update == 0 || time_since_last_observation == 0 {
            return Ok(());
        }
        let delta_token_0_price_x32 = token_0_price_x32
            .checked_mul(time_since_last_update.into())
            .ok_or(ErrorCode::MathOverflow)?;
        let delta_token_1_price_x32 = token_1_price_x32
            .checked_mul(time_since_last_update.into())
            .ok_or(ErrorCode::MathOverflow)?;
        if time_since_last_observation < OBSERVATION_UPDATE_DURATION_DEFAULT {
            self.observations[observation_index as usize].cumulative_token_0_price_x32 =
                last_observation
                    .cumulative_token_0_price_x32
                    .wrapping_add(delta_token_0_price_x32);
            self.observations[observation_index as usize].cumulative_token_1_price_x32 =
                last_observation
                    .cumulative_token_1_price_x32
                    .wrapping_add(delta_token_1_price_x32);
        } else {
            self.observations[next_observation_index as usize].block_timestamp = block_timestamp;
            // cumulative_token_price_x32 only occupies the first 64 bits, and the remaining 64 bits are used to store overflow data
            self.observations[next_observation_index as usize].cumulative_token_0_price_x32 =
                last_observation
                    .cumulative_token_0_price_x32
                    .wrapping_add(delta_token_0_price_x32);
            self.observations[next_observation_index as usize].cumulative_token_1_price_x32 =
                last_observation
                    .cumulative_token_1_price_x32
                    .wrapping_add(delta_token_1_price_x32);
            self.observation_index = next_observation_index;
        }
        self.last_update_timestamp = block_timestamp;
        Ok(())
    }
}

/// Returns the block timestamp truncated to 32 bits, i.e. mod 2**32
///
pub fn block_timestamp() -> u64 {
    Clock::get().unwrap().unix_timestamp as u64 // truncation is desired
}

#[cfg(test)]
pub fn block_timestamp_mock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
pub mod observation_test {
    use super::*;

    #[test]
    fn observation_state_size_test() {
        assert_eq!(
            std::mem::size_of::<ObservationState>(),
            ObservationState::LEN - 8
        )
    }
}
