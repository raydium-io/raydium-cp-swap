//! All fee information, to be used for validation currently

pub const FEE_RATE_DENOMINATOR_VALUE: u32 = 1_000_000;

pub struct Fees {}

fn ceil_div(token_amount: u128, fee_numerator: u128, fee_denominator: u128) -> Option<u128> {
    token_amount
        .checked_mul(u128::from(fee_numerator))
        .unwrap()
        .checked_add(fee_denominator)?
        .checked_sub(1)?
        .checked_div(fee_denominator)
}

/// Helper function for calculating swap fee
pub fn floor_div(token_amount: u128, fee_numerator: u128, fee_denominator: u128) -> Option<u128> {
    Some(
        token_amount
            .checked_mul(fee_numerator)?
            .checked_div(fee_denominator)?,
    )
}

impl Fees {
    /// Calculate the trading fee in trading tokens
    pub fn trading_fee(amount: u128, trade_fee_rate: u32) -> Option<u128> {
        ceil_div(
            amount,
            u128::from(trade_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }

    /// Calculate the owner trading fee in trading tokens
    pub fn protocol_fee(amount: u128, protocol_fee_rate: u32) -> Option<u128> {
        floor_div(
            amount,
            u128::from(protocol_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }

    /// Calculate the owner trading fee in trading tokens
    pub fn fund_fee(amount: u128, fund_fee_rate: u32) -> Option<u128> {
        floor_div(
            amount,
            u128::from(fund_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }
}
