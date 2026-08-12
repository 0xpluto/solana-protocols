//! Pump.fun swap math implementation.
//!
//! This module implements the [`SwapMath`] trait for [`BondingCurve`],
//! containing all the constant product AMM calculations.
//!
//! # Formula
//!
//! Pump.fun uses a constant product formula: `x * y = k`
//!
//! For buys (SOL → tokens):
//! - Fees are deducted from input SOL first
//! - `tokens_out = (token_reserves * sol_in_after_fee) / (sol_reserves + sol_in_after_fee)`
//!
//! For sells (tokens → SOL):
//! - SOL output is calculated first
//! - Fees are deducted from output SOL

use tracing::debug;

use crate::error::{Error, Result};
use crate::events::{SwapOutput, SwapOutputBuilder};
use crate::traits::{SwapAmount, SwapDirection, SwapMath, SwapParams};

use super::constants::{
    CREATOR_FEE_BPS, FEE_DENOMINATOR, PROTOCOL_FEE_BPS, SOL_DECIMALS, TOKEN_DECIMALS,
};
use super::fee_config::PumpfunFees;
use super::state::BondingCurve;

/// Compiled-in fee rates — the pre-config fallback.
///
/// A bonding curve's fee tier is keyed on its **market cap**, which moves
/// continuously as the curve fills, so a fixed pair of rates can only be
/// correct at one point on it. Use [`PumpfunQuote`](super::quote::PumpfunQuote),
/// which reads the on-chain `FeeConfig`; this exists until `PoolState` is
/// replaced by quote bundles.
const COMPILED_IN_FEES: PumpfunFees = PumpfunFees {
    lp_fee_bps: 0,
    protocol_fee_bps: PROTOCOL_FEE_BPS,
    creator_fee_bps: CREATOR_FEE_BPS,
};

impl SwapMath for BondingCurve {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        self.calculate_swap_with_fees(params, &COMPILED_IN_FEES)
    }

    fn spot_price(&self) -> f64 {
        BondingCurve::spot_price(self)
    }

    fn is_active(&self) -> bool {
        BondingCurve::is_active(self)
    }
}

/// Bonding curve swap calculations.
///
/// These methods implement the constant product AMM math for pump.fun.
/// Use via [`SwapMath`] trait for unified handling, or call directly
/// on [`BondingCurve`] for protocol-specific access.
impl BondingCurve {
    /// Price a swap with explicit fee rates.
    ///
    /// The rates are a parameter because they are *state*, not a constant:
    /// pumpfun's `FeeConfig` keys them on the curve's market cap. One curve
    /// implementation, two callers — the bundle supplies chain-read rates,
    /// the bare-account path supplies compiled-in ones.
    pub(crate) fn calculate_swap_with_fees(
        &self,
        params: &SwapParams,
        fees: &PumpfunFees,
    ) -> Result<SwapOutput> {
        match (params.direction, &params.amount) {
            (SwapDirection::Buy, SwapAmount::ExactIn(amount)) => self.calculate_buy(*amount, fees),
            (SwapDirection::Buy, SwapAmount::ExactOut(amount)) => {
                self.calculate_buy_exact_out(*amount, fees)
            }
            (SwapDirection::Sell, SwapAmount::ExactIn(amount)) => {
                self.calculate_sell(*amount, fees)
            }
            (SwapDirection::Sell, SwapAmount::ExactOut(amount)) => {
                self.calculate_sell_exact_out(*amount, fees)
            }
        }
    }

    fn calculate_buy(&self, sol_amount: u64, fees: &PumpfunFees) -> Result<SwapOutput> {
        if self.complete {
            return Err(Error::BondingCurveCompleted);
        }

        if sol_amount == 0 {
            return Err(Error::ZeroAmount);
        }

        // All calculations in u128 to prevent overflow
        let amount_in = sol_amount as u128;
        let reserve_in = self.virtual_sol_reserves as u128;
        let reserve_out = self.virtual_token_reserves as u128;

        // Calculate fees on input amount (ceiling division)
        let protocol_fee =
            (amount_in * u128::from(fees.protocol_fee_bps)).div_ceil(FEE_DENOMINATOR as u128);
        let creator_fee =
            (amount_in * u128::from(fees.creator_fee_bps)).div_ceil(FEE_DENOMINATOR as u128);
        let total_fee = protocol_fee + creator_fee;

        // Amount after fees goes into the curve
        let amount_in_after_fee = amount_in - total_fee;

        // Constant product formula: amount_out = (reserve_out * amount_in) / (reserve_in + amount_in)
        let numerator = reserve_out * amount_in_after_fee;
        let denominator = reserve_in + amount_in_after_fee;
        let amount_out = numerator / denominator;

        // Calculate price impact
        let new_reserve_in = reserve_in + amount_in_after_fee;
        let new_reserve_out = reserve_out - amount_out;
        let post_swap_price = calculate_price(new_reserve_in, new_reserve_out);
        let price_impact_bps = calculate_price_impact(self.spot_price(), post_swap_price);

        // Calculate effective price
        let effective_price = calculate_effective_price(sol_amount, amount_out as u64);

        debug!(
            target: "solana_protocols::pumpfun::math",
            sol_in = sol_amount,
            sol_after_fee = amount_in_after_fee as u64,
            tokens_out = amount_out as u64,
            fee = total_fee as u64,
            price_impact_bps,
            "Buy calculation"
        );

        Ok(SwapOutputBuilder::new()
            .amount_in(sol_amount)
            .amount_out(amount_out as u64)
            .protocol_fee(protocol_fee as u64)
            .lp_fee(creator_fee as u64)
            .price_impact_bps(price_impact_bps)
            .effective_price(effective_price)
            .post_swap_price(post_swap_price)
            .build())
    }

    fn calculate_sell(&self, token_amount: u64, fees: &PumpfunFees) -> Result<SwapOutput> {
        if self.complete {
            return Err(Error::BondingCurveCompleted);
        }

        if token_amount == 0 {
            return Err(Error::ZeroAmount);
        }

        // All calculations in u128 to prevent overflow
        let amount_in = token_amount as u128;
        let reserve_in = self.virtual_token_reserves as u128;
        let reserve_out = self.virtual_sol_reserves as u128;

        // Constant product formula
        let numerator = reserve_out * amount_in;
        let denominator = reserve_in + amount_in;
        let amount_out_before_fees = numerator / denominator;

        // Calculate fees on output amount (ceiling division)
        let protocol_fee = (amount_out_before_fees * u128::from(fees.protocol_fee_bps))
            .div_ceil(FEE_DENOMINATOR as u128);
        let creator_fee = (amount_out_before_fees * u128::from(fees.creator_fee_bps))
            .div_ceil(FEE_DENOMINATOR as u128);
        let total_fee = protocol_fee + creator_fee;

        let amount_out = amount_out_before_fees - total_fee;

        // Calculate price impact
        let new_reserve_in = reserve_in + amount_in;
        let new_reserve_out = reserve_out - amount_out_before_fees;
        let post_swap_price = calculate_price(new_reserve_out, new_reserve_in);
        let price_impact_bps = calculate_price_impact(self.spot_price(), post_swap_price);

        // Calculate effective price
        let effective_price = calculate_effective_price(amount_out as u64, token_amount);

        debug!(
            target: "solana_protocols::pumpfun::math",
            tokens_in = token_amount,
            sol_out_before_fees = amount_out_before_fees as u64,
            sol_out = amount_out as u64,
            fee = total_fee as u64,
            price_impact_bps,
            "Sell calculation"
        );

        Ok(SwapOutputBuilder::new()
            .amount_in(token_amount)
            .amount_out(amount_out as u64)
            .protocol_fee(protocol_fee as u64)
            .lp_fee(creator_fee as u64)
            .price_impact_bps(price_impact_bps)
            .effective_price(effective_price)
            .post_swap_price(post_swap_price)
            .build())
    }

    fn calculate_buy_exact_out(&self, token_amount: u64, fees: &PumpfunFees) -> Result<SwapOutput> {
        if self.complete {
            return Err(Error::BondingCurveCompleted);
        }

        if token_amount == 0 {
            return Err(Error::ZeroAmount);
        }

        let amount_out = token_amount as u128;
        let reserve_in = self.virtual_sol_reserves as u128;
        let reserve_out = self.virtual_token_reserves as u128;

        if amount_out >= reserve_out {
            return Err(Error::InsufficientReserves {
                needed: token_amount,
                available: self.virtual_token_reserves,
            });
        }

        // Inverse constant product: amount_in_after_fee = (reserve_in * amount_out) / (reserve_out - amount_out)
        let denominator = reserve_out - amount_out;
        let numerator = reserve_in * amount_out;
        let amount_in_after_fee = numerator.div_ceil(denominator);

        // Gross up for fees: amount_in = amount_in_after_fee / (1 - fee_rate)
        let total_fee_bps = fees.protocol_fee_bps + fees.creator_fee_bps;
        let denominator_after_fee = (FEE_DENOMINATOR - total_fee_bps) as u128;
        let amount_in =
            (amount_in_after_fee * FEE_DENOMINATOR as u128).div_ceil(denominator_after_fee);

        // Calculate fees from the gross amount
        let protocol_fee =
            (amount_in * u128::from(fees.protocol_fee_bps)).div_ceil(FEE_DENOMINATOR as u128);
        let creator_fee =
            (amount_in * u128::from(fees.creator_fee_bps)).div_ceil(FEE_DENOMINATOR as u128);

        // Calculate price impact
        let new_reserve_in = reserve_in + amount_in_after_fee;
        let new_reserve_out = reserve_out - amount_out;
        let post_swap_price = calculate_price(new_reserve_in, new_reserve_out);
        let price_impact_bps = calculate_price_impact(self.spot_price(), post_swap_price);

        let effective_price = calculate_effective_price(amount_in as u64, token_amount);

        debug!(
            target: "solana_protocols::pumpfun::math",
            tokens_out = token_amount,
            sol_in = amount_in as u64,
            fee = (protocol_fee + creator_fee) as u64,
            "Buy exact out calculation"
        );

        Ok(SwapOutputBuilder::new()
            .amount_in(amount_in as u64)
            .amount_out(token_amount)
            .protocol_fee(protocol_fee as u64)
            .lp_fee(creator_fee as u64)
            .price_impact_bps(price_impact_bps)
            .effective_price(effective_price)
            .post_swap_price(post_swap_price)
            .build())
    }

    fn calculate_sell_exact_out(&self, sol_amount: u64, fees: &PumpfunFees) -> Result<SwapOutput> {
        if self.complete {
            return Err(Error::BondingCurveCompleted);
        }

        if sol_amount == 0 {
            return Err(Error::ZeroAmount);
        }

        // For sells, fees are on output. So we need to gross up the desired output.
        let total_fee_bps = fees.protocol_fee_bps + fees.creator_fee_bps;
        let sol_out_before_fees = (sol_amount as u128 * FEE_DENOMINATOR as u128)
            .div_ceil((FEE_DENOMINATOR - total_fee_bps) as u128);

        let reserve_in = self.virtual_token_reserves as u128;
        let reserve_out = self.virtual_sol_reserves as u128;

        if sol_out_before_fees >= reserve_out {
            return Err(Error::InsufficientReserves {
                needed: sol_out_before_fees as u64,
                available: self.virtual_sol_reserves,
            });
        }

        // Inverse constant product: amount_in = (reserve_in * amount_out) / (reserve_out - amount_out)
        let denominator = reserve_out - sol_out_before_fees;
        let numerator = reserve_in * sol_out_before_fees;
        let tokens_in = numerator.div_ceil(denominator);

        // Calculate fees on output
        let protocol_fee = (sol_out_before_fees * u128::from(fees.protocol_fee_bps))
            .div_ceil(FEE_DENOMINATOR as u128);
        let creator_fee = (sol_out_before_fees * u128::from(fees.creator_fee_bps))
            .div_ceil(FEE_DENOMINATOR as u128);

        // Calculate price impact
        let new_reserve_in = reserve_in + tokens_in;
        let new_reserve_out = reserve_out - sol_out_before_fees;
        let post_swap_price = calculate_price(new_reserve_out, new_reserve_in);
        let price_impact_bps = calculate_price_impact(self.spot_price(), post_swap_price);

        let effective_price = calculate_effective_price(sol_amount, tokens_in as u64);

        debug!(
            target: "solana_protocols::pumpfun::math",
            sol_out = sol_amount,
            tokens_in = tokens_in as u64,
            fee = (protocol_fee + creator_fee) as u64,
            "Sell exact out calculation"
        );

        Ok(SwapOutputBuilder::new()
            .amount_in(tokens_in as u64)
            .amount_out(sol_amount)
            .protocol_fee(protocol_fee as u64)
            .lp_fee(creator_fee as u64)
            .price_impact_bps(price_impact_bps)
            .effective_price(effective_price)
            .post_swap_price(post_swap_price)
            .build())
    }
}

/// Calculate price from reserves (SOL per token).
fn calculate_price(sol_reserves: u128, token_reserves: u128) -> f64 {
    let sol_ui = sol_reserves as f64 / 10f64.powi(SOL_DECIMALS as i32);
    let token_ui = token_reserves as f64 / 10f64.powi(TOKEN_DECIMALS as i32);
    sol_ui / token_ui
}

/// Calculate effective price of a swap (SOL per token).
fn calculate_effective_price(sol_amount: u64, token_amount: u64) -> f64 {
    if token_amount == 0 {
        return 0.0;
    }
    let sol_ui = sol_amount as f64 / 10f64.powi(SOL_DECIMALS as i32);
    let token_ui = token_amount as f64 / 10f64.powi(TOKEN_DECIMALS as i32);
    sol_ui / token_ui
}

/// Calculate price impact in basis points.
fn calculate_price_impact(pre_price: f64, post_price: f64) -> u16 {
    if pre_price == 0.0 {
        return 0;
    }
    let impact = ((post_price - pre_price) / pre_price).abs();
    (impact * 10000.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::state::Legacy;
    use solana_program::pubkey::Pubkey;

    fn test_curve() -> BondingCurve {
        BondingCurve {
            virtual_token_reserves: 1_000_000_000_000_000, // 1B tokens (6 decimals)
            virtual_sol_reserves: 30_000_000_000,          // 30 SOL (9 decimals)
            real_token_reserves: 800_000_000_000_000,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
            creator: Pubkey::new_unique(),
            is_mayhem_mode: Legacy::Present(false),
            is_cashback_coin: Legacy::Present(false),
        }
    }

    #[test]
    fn buy_calculation() {
        let curve = test_curve();
        let sol_in = 1_000_000_000; // 1 SOL

        let result = curve.calculate_buy(sol_in, &COMPILED_IN_FEES).unwrap();

        // Should get some tokens out
        assert!(result.amount_out > 0);

        // Fee should be ~1.25% of input
        let expected_fee = (sol_in as u128 * 125 / 10000) as u64;
        assert!(
            (result.fee as i64 - expected_fee as i64).abs() < 100,
            "Fee {} differs from expected {}",
            result.fee,
            expected_fee
        );

        // Protocol fee should be larger than LP fee (0.95% vs 0.30%)
        assert!(result.protocol_fee > result.lp_fee);
    }

    #[test]
    fn sell_calculation() {
        let curve = test_curve();
        let tokens_in = 1_000_000_000_000; // 1M tokens

        let result = curve.calculate_sell(tokens_in, &COMPILED_IN_FEES).unwrap();

        // Should get some SOL out
        assert!(result.amount_out > 0);
        // Fee should be 1.25% of output
        assert!(result.fee > 0);
    }

    #[test]
    fn completed_curve_rejects_trades() {
        let mut curve = test_curve();
        curve.complete = true;

        assert!(matches!(
            curve.calculate_buy(1_000_000_000, &COMPILED_IN_FEES),
            Err(Error::BondingCurveCompleted)
        ));
        assert!(matches!(
            curve.calculate_sell(1_000_000_000_000, &COMPILED_IN_FEES),
            Err(Error::BondingCurveCompleted)
        ));
    }

    #[test]
    fn zero_amount_rejected() {
        let curve = test_curve();

        assert!(matches!(
            curve.calculate_buy(0, &COMPILED_IN_FEES),
            Err(Error::ZeroAmount)
        ));
        assert!(matches!(
            curve.calculate_sell(0, &COMPILED_IN_FEES),
            Err(Error::ZeroAmount)
        ));
    }

    #[test]
    fn buy_exact_out() {
        let curve = test_curve();
        let tokens_out = 10_000_000_000; // 10K tokens

        let result = curve
            .calculate_buy_exact_out(tokens_out, &COMPILED_IN_FEES)
            .unwrap();

        // Should require some SOL
        assert!(result.amount_in > 0);
        assert_eq!(result.amount_out, tokens_out);

        // Verify the amount_in would give us at least the tokens we want
        let forward = curve
            .calculate_buy(result.amount_in, &COMPILED_IN_FEES)
            .unwrap();

        // Due to rounding, allow up to 0.1% deviation
        let min_acceptable = (tokens_out as u128 * 999) / 1000;
        assert!(
            forward.amount_out >= min_acceptable as u64,
            "forward.amount_out {} should be >= {} (tokens_out={}, amount_in={})",
            forward.amount_out,
            min_acceptable,
            tokens_out,
            result.amount_in
        );
    }

    #[test]
    fn sell_exact_out() {
        let curve = test_curve();
        let sol_out = 100_000_000; // 0.1 SOL

        let result = curve
            .calculate_sell_exact_out(sol_out, &COMPILED_IN_FEES)
            .unwrap();

        // Should require some tokens
        assert!(result.amount_in > 0);
        assert_eq!(result.amount_out, sol_out);

        // Verify the tokens_in would give us at least the SOL we want
        let forward = curve
            .calculate_sell(result.amount_in, &COMPILED_IN_FEES)
            .unwrap();

        // Due to rounding, allow up to 0.1% deviation
        let min_acceptable = (sol_out as u128 * 999) / 1000;
        assert!(
            forward.amount_out >= min_acceptable as u64,
            "forward.amount_out {} should be >= {}",
            forward.amount_out,
            min_acceptable
        );
    }

    #[test]
    fn swap_math_trait() {
        let curve = test_curve();

        // Test via trait
        let params = SwapParams::buy(1_000_000_000);
        let result = curve.calculate_swap(&params).unwrap();
        assert!(result.amount_out > 0);

        let params = SwapParams::sell(1_000_000_000_000);
        let result = curve.calculate_swap(&params).unwrap();
        assert!(result.amount_out > 0);

        // Exact out
        let params = SwapParams::buy_exact_out(10_000_000_000);
        let result = curve.calculate_swap(&params).unwrap();
        assert!(result.amount_in > 0);

        let params = SwapParams::sell_exact_out(100_000_000);
        let result = curve.calculate_swap(&params).unwrap();
        assert!(result.amount_in > 0);
    }

    #[test]
    fn price_impact_increases_with_size() {
        let curve = test_curve();

        let small_swap = curve.calculate_buy(100_000_000, &COMPILED_IN_FEES).unwrap(); // 0.1 SOL
        let large_swap = curve
            .calculate_buy(10_000_000_000, &COMPILED_IN_FEES)
            .unwrap(); // 10 SOL

        assert!(
            large_swap.price_impact_bps > small_swap.price_impact_bps,
            "Large swap impact {} should be > small swap impact {}",
            large_swap.price_impact_bps,
            small_swap.price_impact_bps
        );
    }
}
