use std::ops::{BitAnd, BitOr, BitXor};

use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_LP_MINT_SEED: &str = "pool_lp_mint`";

pub enum PoolStatusBitIndex {
    Deposit,
    Withdraw,
    Swap,
}

#[derive(PartialEq, Eq)]
pub enum PoolStatusBitFlag {
    Enable,
    Disable,
}

#[account(zero_copy(unsafe))]
#[repr(packed)]
#[derive(Default, Debug)]
pub struct PoolState {
    pub bump: [u8; 1],
    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable deposit(vaule is 1), 0: normal
    /// bit1, 1: disable withdraw(vaule is 2), 0: normal
    /// bit2, 1: disable swap(vaule is 4), 0: normal
    pub status: u8,

    // Which config the pool belongs
    pub amm_config: Pubkey,
    pub owner: Pubkey,
    /// Token A
    pub token_0_vault: Pubkey,
    /// Token B
    pub token_1_vault: Pubkey,

    /// Pool tokens are issued when A or B tokens are deposited.
    /// Pool tokens can be withdrawn back to the original A or B token.
    pub lp_mint: Pubkey,
    /// Mint information for token A
    pub token_0_mint: Pubkey,
    /// Mint information for token B
    pub token_1_mint: Pubkey,

    pub lp_mint_decimals: u8,
    /// mint0 and mint1 decimals
    pub mint_0_decimals: u8,
    pub mint_1_decimals: u8,

    /// The amounts of token_0 and token_1 that are owed to the liquidity provider.
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    pub padding: [u64; 32],
}

impl PoolState {
    pub const LEN: usize = 8 + 1 + 1 + 7 * 32 + 1 + 1 + 1 + 8 * 4 + 8 * 16;

    pub fn seeds(&self) -> [&[u8]; 5] {
        [
            &POOL_SEED.as_bytes(),
            self.amm_config.as_ref(),
            self.token_0_mint.as_ref(),
            self.token_1_mint.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn initialize(
        &mut self,
        bump: u8,
        pool_creator: Pubkey,
        amm_config: Pubkey,
        token_0_vault: Pubkey,
        token_1_vault: Pubkey,
        token_0_mint: &InterfaceAccount<Mint>,
        token_1_mint: &InterfaceAccount<Mint>,
        lp_mint: &InterfaceAccount<Mint>,
    ) {
        self.bump = [bump];
        self.amm_config = amm_config.key();
        self.owner = pool_creator.key();
        self.token_0_vault = token_0_vault;
        self.token_1_vault = token_1_vault;
        self.lp_mint = lp_mint.key();
        self.token_0_mint = token_0_mint.key();
        self.token_1_mint = token_1_mint.key();
        self.lp_mint_decimals = lp_mint.decimals;
        self.mint_0_decimals = token_0_mint.decimals;
        self.mint_1_decimals = token_1_mint.decimals;

        self.protocol_fees_token_0 = 0;
        self.protocol_fees_token_1 = 0;
        self.fund_fees_token_0 = 0;
        self.fund_fees_token_1 = 0;
    }

    pub fn set_status(&mut self, status: u8) {
        self.status = status
    }

    pub fn set_status_by_bit(&mut self, bit: PoolStatusBitIndex, flag: PoolStatusBitFlag) {
        let s = u8::from(1) << (bit as u8);
        if flag == PoolStatusBitFlag::Disable {
            self.status = self.status.bitor(s);
        } else {
            let m = u8::from(255).bitxor(s);
            self.status = self.status.bitand(m);
        }
    }

    /// Get status by bit, if it is `noraml` status, return true
    pub fn get_status_by_bit(&self, bit: PoolStatusBitIndex) -> bool {
        let status = u8::from(1) << (bit as u8);
        self.status.bitand(status) == 0
    }

    pub fn vault_amount_without_fee(&self, vault_0: u64, vault_1: u64) -> (u64, u64) {
        (
            vault_0
                .checked_sub(self.protocol_fees_token_0 + self.fund_fees_token_0)
                .unwrap(),
            vault_1
                .checked_sub(self.protocol_fees_token_1 + self.fund_fees_token_1)
                .unwrap(),
        )
    }
}

#[cfg(test)]
pub mod pool_test {
    use super::*;

    mod pool_status_test {
        use super::*;

        #[test]
        fn get_set_status_by_bit() {
            let mut pool_state = PoolState::default();
            pool_state.set_status(4); // 0000100
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Deposit),
                true
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Withdraw),
                true
            );

            // disable -> disable, nothing to change
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Disable);
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );

            // disable -> enable
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Enable);
            assert_eq!(pool_state.get_status_by_bit(PoolStatusBitIndex::Swap), true);

            // enable -> enable, nothing to change
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Enable);
            assert_eq!(pool_state.get_status_by_bit(PoolStatusBitIndex::Swap), true);
            // enable -> disable
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Disable);
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );

            pool_state.set_status(5); // 0000101
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Deposit),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Withdraw),
                true
            );

            pool_state.set_status(7); // 0000111
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Deposit),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Withdraw),
                false
            );

            pool_state.set_status(3); // 0000011
            assert_eq!(pool_state.get_status_by_bit(PoolStatusBitIndex::Swap), true);
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Deposit),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Withdraw),
                false
            );
        }
    }
}
