use crate::curve::fees::{Fees, FEE_RATE_DENOMINATOR_VALUE};
use crate::error::ErrorCode;
use crate::states::AmmConfig;
use crate::utils::math::DownCast;
use anchor_lang::prelude::*;
use anchor_lang::AccountDeserialize;

pub const CREATOR_FEE_SHARE_SEED: &str = "creator_fee_share";

/// Custom protocol share rate of the creator fee for a given (creator, amm_config) pair.
/// When this account exists it overrides `AmmConfig::creator_fee_share_rate`.
#[account]
#[derive(Default, Debug)]
pub struct CreatorFeeShare {
    /// Bump to identify PDA
    pub bump: u8,
    /// The pool creator this share rate applies to
    pub creator: Pubkey,
    /// The amm config this share rate applies to
    pub amm_config: Pubkey,
    /// The share of the creator fee retained by the protocol,
    /// denominated in hundredths of a bip (10^-6)
    pub share_rate: u64,
    /// padding
    pub padding: [u64; 8],
}

impl CreatorFeeShare {
    pub const LEN: usize = 8 + 1 + 32 * 2 + 8 + 8 * 8;
}

/// Resolve the share of the creator fee retained by the protocol.
///
/// The `creator_fee_share` account is always passed by the caller but is not required to
/// exist. When it has not been created the rate configured on `amm_config` is used instead.
pub fn resolve_creator_fee_share_rate(
    creator_fee_share: &UncheckedAccount,
    amm_config: &AmmConfig,
) -> Result<u64> {
    if creator_fee_share.data_is_empty() || creator_fee_share.owner != &crate::id() {
        return Ok(amm_config.creator_fee_share_rate);
    }
    let data = creator_fee_share.try_borrow_data()?;
    // `try_deserialize` checks the account discriminator.
    Ok(CreatorFeeShare::try_deserialize(&mut &data[..])?.share_rate)
}

/// Split the accrued creator fee into the amount owed to the creator and the amount
/// retained by the protocol. Rounding of the protocol share is down, so the dust goes to
/// the creator, matching `Fees::protocol_fee`, `Fees::fund_fee` and
/// `Fees::split_creator_fee`, which also carve a share out of an already accrued fee.
pub fn split_creator_fee_shared_amount(creator_fee: u64, share_rate: u64) -> Result<(u64, u64)> {
    require_gte!(
        FEE_RATE_DENOMINATOR_VALUE,
        share_rate,
        ErrorCode::InvalidInput
    );
    let shared_amount = Fees::creator_fee_shared_amount(u128::from(creator_fee), share_rate)
        .ok_or(ErrorCode::MathOverflow)?
        .to_u64()
        .ok_or(ErrorCode::MathOverflow)?;
    let creator_amount = creator_fee
        .checked_sub(shared_amount)
        .ok_or(ErrorCode::MathOverflow)?;
    Ok((creator_amount, shared_amount))
}

#[cfg(test)]
mod creator_fee_share_test {
    use super::*;

    #[test]
    fn creator_fee_share_size_test() {
        assert_eq!(CreatorFeeShare::LEN, 145);
    }

    #[test]
    fn a_share_rate_above_the_denominator_is_rejected() {
        assert!(split_creator_fee_shared_amount(1_000, FEE_RATE_DENOMINATOR_VALUE + 1).is_err());
    }

    #[test]
    fn zero_share_rate_leaves_the_whole_fee_to_the_creator() {
        assert_eq!(
            split_creator_fee_shared_amount(1_000, 0).unwrap(),
            (1_000, 0)
        );
    }

    #[test]
    fn full_share_rate_leaves_nothing_to_the_creator() {
        assert_eq!(
            split_creator_fee_shared_amount(1_000, FEE_RATE_DENOMINATOR_VALUE).unwrap(),
            (0, 1_000)
        );
    }

    #[test]
    fn split_is_exact_and_rounds_the_shared_amount_down() {
        // 20% of 1_000 is exactly 200
        assert_eq!(
            split_creator_fee_shared_amount(1_000, 200_000).unwrap(),
            (800, 200)
        );
        // 20% of 1 rounds down to 0, the dust goes to the creator
        assert_eq!(split_creator_fee_shared_amount(1, 200_000).unwrap(), (1, 0));
        // 20% of 9 rounds down to 1
        assert_eq!(split_creator_fee_shared_amount(9, 200_000).unwrap(), (8, 1));
        // the smallest possible rate rounds away entirely on a one unit fee
        assert_eq!(split_creator_fee_shared_amount(1, 1).unwrap(), (1, 0));
        // a zero fee stays a zero split
        assert_eq!(split_creator_fee_shared_amount(0, 200_000).unwrap(), (0, 0));
    }

    /// The protocol must never be handed more than the fee itself, which holds because
    /// the share rate is capped at the fee denominator.
    #[test]
    fn the_shared_amount_never_exceeds_the_fee() {
        for fee in [0u64, 1, 7, 999, 1_000_000, u64::MAX] {
            for rate in [0u64, 1, 333_333, 999_999, FEE_RATE_DENOMINATOR_VALUE] {
                let (_, shared_amount) = split_creator_fee_shared_amount(fee, rate).unwrap();
                assert!(shared_amount <= fee);
            }
        }
    }

    #[test]
    fn split_never_loses_or_creates_value() {
        for fee in [0u64, 1, 7, 999, 1_000_000, u64::MAX] {
            for rate in [0u64, 1, 333_333, 500_000, FEE_RATE_DENOMINATOR_VALUE] {
                let (creator_amount, shared_amount) =
                    split_creator_fee_shared_amount(fee, rate).unwrap();
                assert_eq!(creator_amount + shared_amount, fee);
            }
        }
    }
}
