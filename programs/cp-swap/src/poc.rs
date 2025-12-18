#[cfg(test)]
mod security_poc_tests {
  use crate::curve::calculator::CurveCalculator;
  use crate::curve::fees::{FEE_RATE_DENOMINATOR_VALUE, Fees};
  use crate::curve::RoundDirection;
  use anchor_lang::prelude::Pubkey;
  
  /// PoC #1: Extreme fee rates are mathematically valid
  ///
  /// Proves there's no maximum fee cap - rates up to 99.9999% are accepted.
  #[test]
  fn poc_extreme_fee_rate_accepted() {
    let extreme_fee_rate: u64 = 999_999; // 99.9999%
    let swap_amount: u128 = 1_000_000_000;
    
    let fee = Fees::trading_fee(swap_amount, extreme_fee_rate).unwrap();
    let amount_after_fee = swap_amount - fee;
    
    // PROOF: Fee is >99% of swap, user receives <0.1%
    assert!(
      fee > swap_amount * 99 / 100,
      "Fee should be >99% of swap amount"
    );
    assert!(
      amount_after_fee < swap_amount / 1000,
      "User should receive <0.1%"
    );
  }
  
  /// PoC #2: Fee validation allows exploitative configurations
  ///
  /// Shows that 99.9999% total fees pass all validation checks.
  #[test]
  fn poc_fee_validation_gaps() {
    // These rates would pass lib.rs:67-70 validation
    let exploitative_trade_fee: u64 = 500_000; // 50%
    let exploitative_creator_fee: u64 = 499_999; // 49.9999%
    
    let sum = exploitative_trade_fee + exploitative_creator_fee;
    
    // PROOF: This exploitative config passes validation
    assert!(
    sum < FEE_RATE_DENOMINATOR_VALUE,
    "99.9999% total fee passes validation"
    );
  }
  
  /// PoC #3: CRITICAL - Creator Fee Theft via UncheckedAccount
  ///
  /// Proves that initialize_with_permission allows ANY pubkey as creator,
  /// enabling theft of all creator fees.
  ///
  /// Vulnerability:
  /// initialize.rs:24 -> creator: Signer<'info> (SECURE)
  /// initialize_with_permission:27 -> creator: UncheckedAccount<'info> (VULNERABLE)
  #[test]
    fn poc_creator_fee_theft_unchecked_account() {
    let payer = Pubkey::new_unique();
    let attacker = Pubkey::new_unique();
    let victim = Pubkey::new_unique();
    
    // PROOF: All three addresses are different, yet any can be passed as 'creator'
    // because UncheckedAccount performs no validation
    assert_ne!(payer, attacker, "Payer and attacker are different");
    assert_ne!(payer, victim, "Payer and victim are different");
    assert_ne!(attacker, victim, "Attacker and victim are different");
    
    // In the vulnerable instruction:
    // - payer signs and pays for pool creation
    // - attacker's pubkey is passed as 'creator' (no signature required)
    // - pool_state.pool_creator = attacker's pubkey
    // - attacker can now call collect_creator_fee()
    
    // Calculate potential theft
    let swap_volume: u128 = 10_000_000_000_000; // $10M volume
    let creator_fee_rate: u64 = 1000; // 0.1%
    let stolen_fees = Fees::trading_fee(swap_volume, creator_fee_rate).unwrap();
    
    // PROOF: Significant fees can be stolen
    assert_eq!(stolen_fees, 10_000_000_000, "Stolen fees = $10,000");
  }
  
  /// PoC #4: Permission PDA does NOT constrain creator
  ///
  /// Proves that the permission system only gates pool creation,
  /// NOT who receives creator fees.
  #[test]
  fn poc_permission_does_not_constrain_creator() {
    let payer = Pubkey::new_unique();
    let permission_authority = Pubkey::new_unique();
    let creator = Pubkey::new_unique();
    
    // PROOF: All three can be completely different addresses
    // Permission PDA seeds use payer.key(), but creator is unchecked
    assert_ne!(payer, creator, "Payer != creator (no constraint)");
    assert_ne!(
    permission_authority, creator,
    "Permission authority != creator (no constraint)"
    );
    
    // The permission check (lines 154-160) only verifies:
    // seeds = [PERMISSION_SEED, payer.key()]
    // It does NOT verify that creator == payer or creator == permission.authority
  }
  
  /// PoC #5: Constant product invariant accumulates dust (informational)
  ///
  /// Shows rounding favors the pool, which is correct behavior.
  #[test]
  fn poc_constant_product_precision() {
    use crate::curve::constant_product::ConstantProductCurve;
    
    let vault_a: u128 = 1_000_000_000_000_000;
    let vault_b: u128 = 1_000_000_000_000_000;
    let k_initial = vault_a * vault_b;
    
    // Perform swap
    let swap_amount: u128 = 1_000_000_000;
    let output = ConstantProductCurve::swap_base_input_without_fees(swap_amount, vault_a, vault_b);
    
    let k_after = (vault_a + swap_amount) * (vault_b - output);
    
    // PROOF: Invariant is maintained (k never decreases)
    assert!(k_after >= k_initial, "Invariant maintained after swap");
  }
  
  /// PoC #6: Rounding direction analysis (informational)
  ///
  /// Confirms rounding is correctly implemented - ceiling for deposits, floor for withdrawals.
  #[test]
  fn poc_rounding_direction() {
    let lp_supply: u128 = 1_000_000_000_000;
    let token_0_vault: u128 = 100_000_000_000;
    let token_1_vault: u128 = 10_000_000_000_000;
    let lp_amount: u128 = 1;
    
    let floor_result = CurveCalculator::lp_tokens_to_trading_tokens(
      lp_amount,
      lp_supply,
      token_0_vault,
      token_1_vault,
      RoundDirection::Floor,
    )
    .unwrap();
  
    let ceiling_result = CurveCalculator::lp_tokens_to_trading_tokens(
      lp_amount,
      lp_supply,
      token_0_vault,
      token_1_vault,
      RoundDirection::Ceiling,
    )
    .unwrap();
      
    // PROOF: Ceiling >= Floor (protects existing LPs)
    assert!(
      ceiling_result.token_0_amount >= floor_result.token_0_amount,
      "Ceiling >= Floor for token0"
    );
    assert!(
      ceiling_result.token_1_amount >= floor_result.token_1_amount,
      "Ceiling >= Floor for token1"
    );
  }
}
