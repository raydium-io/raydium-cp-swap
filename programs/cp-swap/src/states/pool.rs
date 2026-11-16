use crate::states::creator_fee_share::split_creator_fee_shared_amount;
use crate::{curve::TradeDirection, error::ErrorCode};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use std::ops::{BitAnd, BitOr, BitXor};
/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_LP_MINT_SEED: &str = "pool_lp_mint";
pub const POOL_VAULT_SEED: &str = "pool_vault";

pub const Q32: u128 = (u32::MAX as u128) + 1; // 2^32

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

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum CreatorFeeOn {
    /// Both token0 and token1 can be used as trade fees.
    /// It depends on what the input token is.
    BothToken,
    /// Only token0 can be used as trade fees.
    OnlyToken0,
    /// Only token1 can be used as trade fees.
    OnlyToken1,
}

impl CreatorFeeOn {
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(CreatorFeeOn::BothToken),
            1 => Ok(CreatorFeeOn::OnlyToken0),
            2 => Ok(CreatorFeeOn::OnlyToken1),
            _ => Err(ErrorCode::InvalidFeeModel.into()),
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            CreatorFeeOn::BothToken => 0u8,
            CreatorFeeOn::OnlyToken0 => 1u8,
            CreatorFeeOn::OnlyToken1 => 2u8,
        }
    }
}

pub struct SwapParams {
    pub trade_direction: TradeDirection,
    pub total_input_token_amount: u64,
    pub total_output_token_amount: u64,
    pub token_0_price_x64: u128,
    pub token_1_price_x64: u128,
    pub is_creator_fee_on_input: bool,
}

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct PoolState {
    /// Which config the pool belongs
    pub amm_config: Pubkey,
    /// pool creator
    pub pool_creator: Pubkey,
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

    /// token_0 program
    pub token_0_program: Pubkey,
    /// token_1 program
    pub token_1_program: Pubkey,

    /// observation account to store oracle data
    pub observation_key: Pubkey,

    pub auth_bump: u8,
    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable deposit(value is 1), 0: normal
    /// bit1, 1: disable withdraw(value is 2), 0: normal
    /// bit2, 1: disable swap(value is 4), 0: normal
    pub status: u8,

    pub lp_mint_decimals: u8,
    /// mint0 and mint1 decimals
    pub mint_0_decimals: u8,
    pub mint_1_decimals: u8,

    /// True circulating supply without burns and lock ups
    pub lp_supply: u64,
    /// The amounts of token_0 and token_1 that are owed to the liquidity provider.
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    /// The timestamp allowed for swap in the pool.
    pub open_time: u64,
    /// recent epoch
    pub recent_epoch: u64,

    /// Creator fee collect mode
    /// 0: both token_0 and token_1 can be used as trade fees. It depends on what the input token is when swapping
    /// 1: only token_0 as trade fee
    /// 2: only token_1 as trade fee
    pub creator_fee_on: u8,
    pub enable_creator_fee: bool,
    pub padding1: [u8; 6],
    pub creator_fees_token_0: u64,
    pub creator_fees_token_1: u64,
    /// padding for future updates
    pub padding: [u64; 28],
}

impl PoolState {
    pub const LEN: usize = 8 + 10 * 32 + 1 * 5 + 8 * 7 + 1 * 2 + 6 * 1 + 2 * 8 + 8 * 28;

    pub fn initialize(
        &mut self,
        auth_bump: u8,
        lp_supply: u64,
        open_time: u64,
        pool_creator: Pubkey,
        amm_config: Pubkey,
        token_0_vault: Pubkey,
        token_1_vault: Pubkey,
        token_0_mint: &InterfaceAccount<Mint>,
        token_1_mint: &InterfaceAccount<Mint>,
        lp_mint: Pubkey,
        lp_mint_decimals: u8,
        observation_key: Pubkey,
        creator_fee_on: CreatorFeeOn,
        enable_creator_fee: bool,
    ) {
        self.amm_config = amm_config.key();
        self.pool_creator = pool_creator.key();
        self.token_0_vault = token_0_vault;
        self.token_1_vault = token_1_vault;
        self.lp_mint = lp_mint.key();
        self.token_0_mint = token_0_mint.key();
        self.token_1_mint = token_1_mint.key();
        self.token_0_program = *token_0_mint.to_account_info().owner;
        self.token_1_program = *token_1_mint.to_account_info().owner;
        self.observation_key = observation_key;
        self.auth_bump = auth_bump;
        self.lp_mint_decimals = lp_mint_decimals;
        self.mint_0_decimals = token_0_mint.decimals;
        self.mint_1_decimals = token_1_mint.decimals;
        self.lp_supply = lp_supply;
        self.protocol_fees_token_0 = 0;
        self.protocol_fees_token_1 = 0;
        self.fund_fees_token_0 = 0;
        self.fund_fees_token_1 = 0;
        self.open_time = open_time;
        self.recent_epoch = Clock::get().unwrap().epoch;
        self.creator_fee_on = creator_fee_on.to_u8();
        self.enable_creator_fee = enable_creator_fee;
        self.padding1 = [0u8; 6];
        self.creator_fees_token_0 = 0;
        self.creator_fees_token_1 = 0;
        self.padding = [0u64; 28];
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

    /// Settle the accrued creator fee at `share_rate`: the protocol's share is booked on
    /// the pool and stays in the vault until the admin collects it through
    /// `collect_protocol_fee`, and the amounts owed to the creator are returned.
    pub fn settle_creator_fee(&mut self, share_rate: u64) -> Result<(u64, u64)> {
        let (creator_amount_0, shared_amount_0) =
            split_creator_fee_shared_amount(self.creator_fees_token_0, share_rate)?;
        let (creator_amount_1, shared_amount_1) =
            split_creator_fee_shared_amount(self.creator_fees_token_1, share_rate)?;

        let protocol_fees_token_0 = self.protocol_fees_token_0;
        let protocol_fees_token_1 = self.protocol_fees_token_1;
        self.protocol_fees_token_0 = protocol_fees_token_0
            .checked_add(shared_amount_0)
            .ok_or(ErrorCode::MathOverflow)?;
        self.protocol_fees_token_1 = protocol_fees_token_1
            .checked_add(shared_amount_1)
            .ok_or(ErrorCode::MathOverflow)?;

        self.creator_fees_token_0 = 0;
        self.creator_fees_token_1 = 0;

        Ok((creator_amount_0, creator_amount_1))
    }

    pub fn vault_amount_without_fee(&self, vault_0: u64, vault_1: u64) -> Result<(u64, u64)> {
        let fees_token_0 = self
            .protocol_fees_token_0
            .checked_add(self.fund_fees_token_0)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_add(self.creator_fees_token_0)
            .ok_or(ErrorCode::MathOverflow)?;
        let fees_token_1 = self
            .protocol_fees_token_1
            .checked_add(self.fund_fees_token_1)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_add(self.creator_fees_token_1)
            .ok_or(ErrorCode::MathOverflow)?;
        Ok((
            vault_0
                .checked_sub(fees_token_0)
                .ok_or(ErrorCode::InsufficientVault)?,
            vault_1
                .checked_sub(fees_token_1)
                .ok_or(ErrorCode::InsufficientVault)?,
        ))
    }

    pub fn token_price_x32(&self, vault_0: u64, vault_1: u64) -> Result<(u128, u128)> {
        let (token_0_amount, token_1_amount) = self.vault_amount_without_fee(vault_0, vault_1)?;
        Ok((
            token_1_amount as u128 * Q32 as u128 / token_0_amount as u128,
            token_0_amount as u128 * Q32 as u128 / token_1_amount as u128,
        ))
    }

    pub fn update_lp_supply(
        &mut self,
        liquidity_delta: u64,
        add: bool,
        recent_epoch: u64,
    ) -> Result<()> {
        if add {
            self.lp_supply = self
                .lp_supply
                .checked_add(liquidity_delta)
                .ok_or(ErrorCode::MathOverflow)?;
        } else {
            self.lp_supply = self
                .lp_supply
                .checked_sub(liquidity_delta)
                .ok_or(ErrorCode::MathOverflow)?;
        }
        self.recent_epoch = recent_epoch;
        Ok(())
    }

    // Determine the method used by the creator to calculate transaction fees
    pub fn is_creator_fee_on_input(&self, direction: TradeDirection) -> Result<bool> {
        let fee_on = CreatorFeeOn::from_u8(self.creator_fee_on)?;
        Ok(match (fee_on, direction) {
            (CreatorFeeOn::BothToken, _) => true,
            (CreatorFeeOn::OnlyToken0, TradeDirection::ZeroForOne) => true,
            (CreatorFeeOn::OnlyToken1, TradeDirection::OneForZero) => true,
            _ => false,
        })
    }

    pub fn get_swap_params(
        &self,
        input_vault_key: Pubkey,
        output_vault_key: Pubkey,
        input_vault_amount: u64,
        output_vault_amount: u64,
    ) -> Result<SwapParams> {
        let (
            trade_direction,
            total_input_token_amount,
            total_output_token_amount,
            token_0_price_x64,
            token_1_price_x64,
            is_creator_fee_on_input,
        ) = if input_vault_key == self.token_0_vault && output_vault_key == self.token_1_vault {
            let (total_input_token_amount, total_output_token_amount) =
                self.vault_amount_without_fee(input_vault_amount, output_vault_amount)?;
            let (token_0_price_x64, token_1_price_x64) =
                self.token_price_x32(input_vault_amount, output_vault_amount)?;

            (
                TradeDirection::ZeroForOne,
                total_input_token_amount,
                total_output_token_amount,
                token_0_price_x64,
                token_1_price_x64,
                self.is_creator_fee_on_input(TradeDirection::ZeroForOne)?,
            )
        } else if input_vault_key == self.token_1_vault && output_vault_key == self.token_0_vault {
            let (total_output_token_amount, total_input_token_amount) =
                self.vault_amount_without_fee(output_vault_amount, input_vault_amount)?;
            let (token_0_price_x64, token_1_price_x64) =
                self.token_price_x32(output_vault_amount, input_vault_amount)?;

            (
                TradeDirection::OneForZero,
                total_input_token_amount,
                total_output_token_amount,
                token_0_price_x64,
                token_1_price_x64,
                self.is_creator_fee_on_input(TradeDirection::OneForZero)?,
            )
        } else {
            return err!(ErrorCode::InvalidVault);
        };
        Ok(SwapParams {
            trade_direction,
            total_input_token_amount,
            total_output_token_amount,
            token_0_price_x64,
            token_1_price_x64,
            is_creator_fee_on_input,
        })
    }

    pub fn adjust_creator_fee_rate(&self, creator_fee_rate: u64) -> u64 {
        if self.enable_creator_fee {
            creator_fee_rate
        } else {
            0
        }
    }

    pub fn update_fees(
        &mut self,
        protocol_fee: u64,
        fund_fee: u64,
        creator_fee: u64,
        direction: TradeDirection,
    ) -> Result<()> {
        if !self.enable_creator_fee {
            require_eq!(creator_fee, 0)
        }
        let is_creator_fee_on_input = self.is_creator_fee_on_input(direction)?;
        match direction {
            TradeDirection::ZeroForOne => {
                self.protocol_fees_token_0 = self
                    .protocol_fees_token_0
                    .checked_add(protocol_fee)
                    .unwrap();
                self.fund_fees_token_0 = self.fund_fees_token_0.checked_add(fund_fee).unwrap();

                if is_creator_fee_on_input {
                    self.creator_fees_token_0 =
                        self.creator_fees_token_0.checked_add(creator_fee).unwrap();
                } else {
                    self.creator_fees_token_1 =
                        self.creator_fees_token_1.checked_add(creator_fee).unwrap();
                }
            }
            TradeDirection::OneForZero => {
                self.protocol_fees_token_1 = self
                    .protocol_fees_token_1
                    .checked_add(protocol_fee)
                    .unwrap();
                self.fund_fees_token_1 = self.fund_fees_token_1.checked_add(fund_fee).unwrap();
                if is_creator_fee_on_input {
                    self.creator_fees_token_1 =
                        self.creator_fees_token_1.checked_add(creator_fee).unwrap();
                } else {
                    self.creator_fees_token_0 =
                        self.creator_fees_token_0.checked_add(creator_fee).unwrap();
                }
            }
        };
        Ok(())
    }
}

#[cfg(test)]
pub mod pool_test {
    use super::*;

    #[test]
    fn pool_state_size_test() {
        assert_eq!(std::mem::size_of::<PoolState>(), PoolState::LEN - 8)
    }

    /// The shared creator fee fields are carved out of the existing padding, so the
    /// account length must stay the same for already deployed pools to keep loading.
    #[test]
    fn pool_state_size_is_unchanged() {
        assert_eq!(PoolState::LEN, 637);
    }

    /// A packed struct cannot hand out references to its fields, so the assertions read
    /// the values into locals first.
    #[test]
    fn settle_creator_fee_books_the_shared_part_and_accumulates() {
        let mut pool_state = PoolState::default();
        pool_state.creator_fees_token_0 = 1_000;
        pool_state.creator_fees_token_1 = 9;

        // 20% of 1_000 is 200; 20% of 9 rounds down to 1
        let owed = pool_state.settle_creator_fee(200_000).unwrap();
        assert_eq!(owed, (800, 8));
        let fees = (
            pool_state.creator_fees_token_0,
            pool_state.creator_fees_token_1,
            pool_state.protocol_fees_token_0,
            pool_state.protocol_fees_token_1,
        );
        assert_eq!(fees, (0, 0, 200, 1));

        // a second settlement adds to what is already booked
        pool_state.creator_fees_token_0 = 500;
        assert_eq!(pool_state.settle_creator_fee(200_000).unwrap(), (400, 0));
        let shared_0 = pool_state.protocol_fees_token_0;
        assert_eq!(shared_0, 300);
    }

    #[test]
    fn settle_creator_fee_reports_overflow_of_the_booked_share() {
        let mut pool_state = PoolState::default();
        pool_state.protocol_fees_token_0 = u64::MAX;
        pool_state.creator_fees_token_0 = 1_000;

        assert!(pool_state.settle_creator_fee(200_000).is_err());
    }

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

#[cfg(test)]
mod solvency_prop {
    use super::*;
    use crate::curve::calculator::{CurveCalculator, RoundDirection, TradeDirection};
    use crate::curve::fees::FEE_RATE_DENOMINATOR_VALUE as D;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum Op {
        SwapIn(bool, u64),
        SwapOut(bool, u64),
        CollectProtocol,
        CollectCreator(u64),
        Deposit(u64),
        Withdraw(u64),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (any::<bool>(), 1u64..(1u64 << 40)).prop_map(|(a, b)| Op::SwapIn(a, b)),
            (any::<bool>(), 1u64..(1u64 << 40)).prop_map(|(a, b)| Op::SwapOut(a, b)),
            Just(Op::CollectProtocol),
            (0u64..=D).prop_map(Op::CollectCreator),
            (1u64..(1u64 << 36)).prop_map(Op::Deposit),
            (1u64..(1u64 << 36)).prop_map(Op::Withdraw),
        ]
    }

    fn isqrt(n: u128) -> u128 {
        if n == 0 { return 0; }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x { x = y; y = (x + n / x) / 2; }
        x
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20000))]
        #[test]
        fn pool_stays_solvent(
            v0 in 1_000u64..(1u64 << 42),
            v1 in 1_000u64..(1u64 << 42),
            trade in 0u64..=200_000,
            creator in 0u64..=200_000,
            protocol in 0u64..=D,
            fund_pct in 0u64..=100,
            enable in any::<bool>(),
            fee_on in 0u8..3,
            ops in proptest::collection::vec(op(), 1..40),
        ) {
            let fund = (D - protocol) * fund_pct / 100;
            let mut pool = PoolState::default();
            let k0 = Pubkey::new_unique();
            let k1 = Pubkey::new_unique();
            pool.token_0_vault = k0;
            pool.token_1_vault = k1;
            pool.enable_creator_fee = enable;
            pool.creator_fee_on = fee_on;
            let mut vault0 = v0 as u128;
            let mut vault1 = v1 as u128;
            let lp0 = isqrt(vault0 * vault1);
            prop_assume!(lp0 > 200);
            pool.lp_supply = lp0 as u64;
            let c_rate = pool.adjust_creator_fee_rate(creator);

            for o in ops {
                let reserves = |p: &PoolState, a: u128, b: u128| -> Option<(u128, u128)> {
                    p.vault_amount_without_fee(a as u64, b as u64).ok().map(|(x, y)| (x as u128, y as u128))
                };
                let (r0, r1) = reserves(&pool, vault0, vault1).expect("I1 fees exceed vault");
                match o {
                    Op::SwapIn(zfo, amt) => {
                        let (ik, ok, iv, ov) = if zfo { (k0, k1, vault0, vault1) } else { (k1, k0, vault1, vault0) };
                        let Ok(p) = pool.get_swap_params(ik, ok, iv as u64, ov as u64) else { continue };
                        let dir = p.trade_direction;
                        let Some(res) = CurveCalculator::swap_base_input(
                            amt as u128, p.total_input_token_amount as u128, p.total_output_token_amount as u128,
                            trade, c_rate, protocol, fund, p.is_creator_fee_on_input) else { continue };
                        let before = (p.total_input_token_amount as u128) * (p.total_output_token_amount as u128);
                        let after = res.new_input_vault_amount * res.new_output_vault_amount;
                        if after < before || res.output_amount == 0 || res.output_amount > ov { continue; }
                        pool.update_fees(res.protocol_fee as u64, res.fund_fee as u64, res.creator_fee as u64, dir).unwrap();
                        if zfo { vault0 += amt as u128; vault1 -= res.output_amount; } else { vault1 += amt as u128; vault0 -= res.output_amount; }
                        let (n0, n1) = reserves(&pool, vault0, vault1).expect("I1 after swap_in");
                        prop_assert!(n0 * n1 >= r0 * r1, "I2 k decreased (in) {:?}", dir);
                    }
                    Op::SwapOut(zfo, amt) => {
                        let (ik, ok, iv, ov) = if zfo { (k0, k1, vault0, vault1) } else { (k1, k0, vault1, vault0) };
                        let Ok(p) = pool.get_swap_params(ik, ok, iv as u64, ov as u64) else { continue };
                        let dir = p.trade_direction;
                        let tout = p.total_output_token_amount as u128;
                        if (amt as u128) >= tout / 2 { continue; }
                        let Some(res) = CurveCalculator::swap_base_output(
                            amt as u128, p.total_input_token_amount as u128, tout,
                            trade, c_rate, protocol, fund, p.is_creator_fee_on_input) else { continue };
                        let before = (p.total_input_token_amount as u128) * tout;
                        let after = res.new_input_vault_amount * res.new_output_vault_amount;
                        if after < before || res.input_amount == 0 || res.output_amount > ov { continue; }
                        pool.update_fees(res.protocol_fee as u64, res.fund_fee as u64, res.creator_fee as u64, dir).unwrap();
                        if zfo { vault0 += res.input_amount; vault1 -= res.output_amount; } else { vault1 += res.input_amount; vault0 -= res.output_amount; }
                        let (n0, n1) = reserves(&pool, vault0, vault1).expect("I1 after swap_out");
                        prop_assert!(n0 * n1 >= r0 * r1, "I2 k decreased (out) {:?}", dir);
                    }
                    Op::CollectProtocol => {
                        let (p0, p1, f0, f1) = (pool.protocol_fees_token_0, pool.protocol_fees_token_1, pool.fund_fees_token_0, pool.fund_fees_token_1);
                        vault0 -= (p0 + f0) as u128; vault1 -= (p1 + f1) as u128;
                        pool.protocol_fees_token_0 = 0; pool.protocol_fees_token_1 = 0;
                        pool.fund_fees_token_0 = 0; pool.fund_fees_token_1 = 0;
                        let (n0, n1) = reserves(&pool, vault0, vault1).unwrap();
                        prop_assert_eq!((n0, n1), (r0, r1), "I3 collect_protocol changed reserves");
                    }
                    Op::CollectCreator(rate) => {
                        let Ok((a0, a1)) = pool.settle_creator_fee(rate) else { continue };
                        vault0 -= a0 as u128; vault1 -= a1 as u128;
                        let (n0, n1) = reserves(&pool, vault0, vault1).expect("I1 after collect_creator");
                        prop_assert_eq!((n0, n1), (r0, r1), "I3 collect_creator changed reserves");
                    }
                    Op::Deposit(lp) => {
                        let s = pool.lp_supply as u128;
                        let Some(t) = CurveCalculator::lp_tokens_to_trading_tokens(lp as u128, s, r0, r1, RoundDirection::Ceiling) else { continue };
                        if t.token_0_amount == 0 || t.token_1_amount == 0 { continue; }
                        prop_assert!(t.token_0_amount * s >= (lp as u128) * r0 && t.token_1_amount * s >= (lp as u128) * r1, "I4 deposit undercharged");
                        vault0 += t.token_0_amount; vault1 += t.token_1_amount;
                        pool.lp_supply += lp;
                    }
                    Op::Withdraw(lp) => {
                        let s = pool.lp_supply as u128;
                        if (lp as u128) + 100 >= s { continue; }
                        let Some(t) = CurveCalculator::lp_tokens_to_trading_tokens(lp as u128, s, r0, r1, RoundDirection::Floor) else { continue };
                        if t.token_0_amount == 0 || t.token_1_amount == 0 { continue; }
                        prop_assert!(t.token_0_amount * s <= (lp as u128) * r0 && t.token_1_amount * s <= (lp as u128) * r1, "I4 withdraw overpaid");
                        vault0 -= t.token_0_amount; vault1 -= t.token_1_amount;
                        pool.lp_supply -= lp;
                    }
                }
            }
        }
    }
}
