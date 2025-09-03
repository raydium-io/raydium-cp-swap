//! All fee information, to be used for validation currently

pub const FEE_RATE_DENOMINATOR_VALUE: u64 = 1_000_000;

pub struct Fees {}

fn ceil_div(token_amount: u128, fee_numerator: u128, fee_denominator: u128) -> Option<u128> {
    if fee_denominator == 0 {
        return None;
    }
    token_amount
        .checked_mul(u128::from(fee_numerator))?
        .checked_add(fee_denominator)?
        .checked_sub(1)?
        .checked_div(fee_denominator)
}

/// Helper function for calculating swap fee
pub fn floor_div(token_amount: u128, fee_numerator: u128, fee_denominator: u128) -> Option<u128> {
    if fee_denominator == 0 {
        return None;
    }
    token_amount
        .checked_mul(fee_numerator)?
        .checked_div(fee_denominator)
}

impl Fees {
    /// Calculate the trading fee in trading tokens
    pub fn trading_fee(amount: u128, trade_fee_rate: u64) -> Option<u128> {
        ceil_div(
            amount,
            u128::from(trade_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }

    /// Calculate the owner protocol fee in trading tokens
    pub fn protocol_fee(amount: u128, protocol_fee_rate: u64) -> Option<u128> {
        floor_div(
            amount,
            u128::from(protocol_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }

    /// Calculate the owner fund fee in trading tokens
    pub fn fund_fee(amount: u128, fund_fee_rate: u64) -> Option<u128> {
        floor_div(
            amount,
            u128::from(fund_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }

    /// Calculate the creator fee
    pub fn creator_fee(amount: u128, creator_fee_rate: u64) -> Option<u128> {
        ceil_div(
            amount,
            u128::from(creator_fee_rate),
            u128::from(FEE_RATE_DENOMINATOR_VALUE),
        )
    }

    pub fn split_creator_fee(
        total_fee: u128,
        trade_fee_rate: u64,
        creator_fee_rate: u64,
    ) -> Option<u128> {
        floor_div(
            total_fee,
            u128::from(creator_fee_rate),
            u128::from(trade_fee_rate + creator_fee_rate),
        )
    }

    pub fn calculate_pre_fee_amount(post_fee_amount: u128, trade_fee_rate: u64) -> Option<u128> {
        if trade_fee_rate == 0 {
            Some(post_fee_amount)
        } else {
            let numerator = post_fee_amount.checked_mul(u128::from(FEE_RATE_DENOMINATOR_VALUE))?;
            let denominator =
                u128::from(FEE_RATE_DENOMINATOR_VALUE).checked_sub(u128::from(trade_fee_rate))?;

            numerator
                .checked_add(denominator)?
                .checked_sub(1)?
                .checked_div(denominator)
        }
    }
}
