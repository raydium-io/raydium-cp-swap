use anchor_lang::prelude::*;

pub const AMM_CONFIG_SEED: &str = "amm_config";

/// Holds the current owner of the factory
#[account]
#[derive(Default, Debug)]
pub struct AmmConfig {
    /// Bump to identify PDA
    pub bump: u8,
    pub index: u16,
    /// The trade fee, denominated in hundredths of a bip (10^-6)
    pub trade_fee_rate: u32,
    /// The protocol fee
    pub protocol_fee_rate: u32,
    /// The fund fee, denominated in hundredths of a bip (10^-6)
    pub fund_fee_rate: u32,
    /// Address of the protocol fee owner
    pub protocol_owner: Pubkey,
    /// Address of the fund fee owner
    pub fund_owner: Pubkey,
    /// padding
    pub padding: [u64; 16],
}

impl AmmConfig {
    pub const LEN: usize = 8 + 1 + 2 + 8 + 8 + 8 + 32 + 32 + 64 * 4;
}
