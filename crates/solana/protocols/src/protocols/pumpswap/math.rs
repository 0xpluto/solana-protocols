//! PumpSwap swap math.
//!
//! Constant product AMM: `x * y = k`
//!
//! Fees are deducted from the output amount:
//! ```text
//! amount_out_before_fees = (reserve_out * amount_in) / (reserve_in + amount_in)
//! total_fee = creator_fee + protocol_fee + lp_fee
//! amount_out = amount_out_before_fees - total_fee
//! ```

use super::state::PoolWithReserves;
use crate::events::SwapOutput;
use crate::traits::{SwapMath, SwapParams};
use crate::Result;
use tracing::warn;

/// Fee structure for a PumpSwap pool.
///
/// Fees are in basis points (1 bps = 0.01%).
#[derive(Debug, Clone, Copy)]
pub struct FeeStructure {
    /// Creator fee in basis points.
    pub creator_bps: u64,
    /// Protocol fee in basis points.
    pub protocol_bps: u64,
    /// LP fee in basis points.
    pub lp_bps: u64,
}

impl FeeStructure {
    /// Total fee in basis points.
    #[must_use]
    pub fn total_bps(&self) -> u64 {
        self.creator_bps + self.protocol_bps + self.lp_bps
    }

    /// Calculate fees from an output amount.
    ///
    /// **Every component rounds up.** Measured 2026-08-10 against 302 executed
    /// swaps on no-creator pools, where the rate is a known-flat 30bp (lp 25 +
    /// protocol 5): flooring protocol and lp — as this did — left the fee 2
    /// units short on 226 of them, since two independent floors each lose up
    /// to one unit. Ceiling all three reproduces the chain exactly on the same
    /// 226. The remainder are a separate, larger-error population.
    ///
    /// The direction matters beyond the rounding: a fee computed too *low*
    /// makes a quote promise more output than the pool will deliver, so the
    /// error lands on the side that fails a swap rather than merely
    /// under-filling it.
    pub fn calculate_fees(&self, amount_before_fees: u64) -> (u64, u64, u64) {
        let amount = amount_before_fees as u128;
        let creator_fee = ceil_div(amount * self.creator_bps as u128, 10000);
        let protocol_fee = ceil_div(amount * self.protocol_bps as u128, 10000);
        let lp_fee = ceil_div(amount * self.lp_bps as u128, 10000);
        (creator_fee as u64, protocol_fee as u64, lp_fee as u64)
    }
}

#[cfg(test)]
mod fee_rounding_tests {
    use super::FeeStructure;

    /// Each component rounds up independently, and the flat case is the one
    /// measured against chain.
    ///
    /// `out_before = 1_000_001` with lp 25 / protocol 5 / creator 0: both
    /// components have a remainder, so flooring both loses 2 units — exactly
    /// the discrepancy seen on 226 of 302 executed no-creator swaps.
    #[test]
    fn every_fee_component_rounds_up() {
        let flat = FeeStructure {
            creator_bps: 0,
            protocol_bps: 5,
            lp_bps: 25,
        };
        let (creator, protocol, lp) = flat.calculate_fees(1_000_001);
        assert_eq!(creator, 0, "no creator, no creator fee");
        assert_eq!(protocol, 501, "5bp of 1_000_001 = 500.0005, rounded up");
        assert_eq!(lp, 2501, "25bp of 1_000_001 = 2500.0025, rounded up");
        // Flooring both would have yielded 500 + 2500 = 3000, two short.
        assert_eq!(creator + protocol + lp, 3002);
    }

    /// An exact multiple must not be inflated by the ceiling.
    #[test]
    fn an_exact_multiple_rounds_to_itself() {
        let flat = FeeStructure {
            creator_bps: 0,
            protocol_bps: 5,
            lp_bps: 25,
        };
        let (_, protocol, lp) = flat.calculate_fees(1_000_000);
        assert_eq!(protocol, 500);
        assert_eq!(lp, 2500);
    }
}

/// Determine fee structure based on SOL liquidity level.
///
/// Fees dynamically adjust based on pool's SOL reserves.
/// Higher liquidity = lower creator fees.
pub fn fee_structure(quote_reserves_sol: f64, has_creator: bool) -> FeeStructure {
    if !has_creator {
        // Bonding curve mode (no creator)
        return FeeStructure {
            creator_bps: 0,
            protocol_bps: 5,
            lp_bps: 25,
        };
    }

    // Dynamic fees based on SOL liquidity level
    // Values from the on-chain fee schedule
    let creator_bps = match quote_reserves_sol as u64 {
        0..=420 => 30,
        421..=1_470 => 95,
        1_471..=2_460 => 90,
        2_461..=3_440 => 85,
        3_441..=4_420 => 80,
        4_421..=9_820 => 75,
        9_821..=14_740 => 70,
        14_741..=19_650 => 65,
        19_651..=24_560 => 60,
        24_561..=29_470 => 55,
        29_471..=34_380 => 50,
        34_381..=39_300 => 45,
        39_301..=44_210 => 40,
        44_211..=49_120 => 35,
        49_121..=54_030 => 30,
        54_031..=58_940 => 27,
        58_941..=63_860 => 25,
        63_861..=68_770 => 22,
        68_771..=73_681 => 20,
        73_682..=78_590 => 17,
        78_591..=83_500 => 15,
        83_501..=88_400 => 12,
        88_401..=93_330 => 10,
        93_331..=98_240 => 7,
        _ => 5, // 98,241+ SOL
    };

    // Protocol and LP fees are constant in swap mode
    let (protocol_bps, lp_bps) = if quote_reserves_sol <= 420.0 {
        (93, 2) // Launch phase
    } else {
        (5, 20) // Standard mode
    };

    FeeStructure {
        creator_bps,
        protocol_bps,
        lp_bps,
    }
}

/// **Legacy path — prefer [`PumpSwapQuote`](super::quote::PumpSwapQuote).**
///
/// Implemented on the bare pool account, so it can reach no other account:
/// reserves must be injected by `set_reserves` (a step the caller has to
/// remember; `from_account_data` zeroes them) and fee rates come from the
/// hardcoded `fee_structure` ladder rather than the on-chain config we cache.
/// Retained only until the RPC assembly in `trading-execution` moves to the
/// bundle — until it does, this path is the one the swap verifier does **not**
/// grade, which is precisely why it should not survive.
impl SwapMath for PoolWithReserves {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        if !self.is_active() {
            return Err(crate::Error::swap_error("PumpSwap pool has no reserves"));
        }

        let quote_reserves_sol = self.quote_reserves() as f64 / 1e9;
        let fees = fee_structure(quote_reserves_sol, self.pool().has_creator());

        match (params.direction, params.amount.is_exact_in()) {
            // Buy exact in: user sends SOL (quote), receives tokens (base)
            (crate::traits::SwapDirection::Buy, true) => {
                let amount_in = params.amount.amount();
                let (_amount_out, output) = calculate_swap_exact_in(
                    amount_in,
                    self.quote_reserves(),
                    self.base_reserves(),
                    &fees,
                )?;
                Ok(output)
            }
            // Sell exact in: user sends tokens (base), receives SOL (quote)
            (crate::traits::SwapDirection::Sell, true) => {
                let amount_in = params.amount.amount();
                let (_amount_out, output) = calculate_swap_exact_in(
                    amount_in,
                    self.base_reserves(),
                    self.quote_reserves(),
                    &fees,
                )?;
                Ok(output)
            }
            // Buy exact out: calculate how much SOL needed for X tokens
            (crate::traits::SwapDirection::Buy, false) => {
                let amount_out = params.amount.amount();
                let output = calculate_swap_exact_out(
                    amount_out,
                    self.quote_reserves(),
                    self.base_reserves(),
                    &fees,
                );
                Ok(output)
            }
            // Sell exact out: calculate how many tokens needed for X SOL
            (crate::traits::SwapDirection::Sell, false) => {
                let amount_out = params.amount.amount();
                let output = calculate_swap_exact_out(
                    amount_out,
                    self.base_reserves(),
                    self.quote_reserves(),
                    &fees,
                );
                Ok(output)
            }
        }
    }

    fn spot_price(&self) -> f64 {
        Self::spot_price(self)
    }

    fn is_active(&self) -> bool {
        Self::is_active(self)
    }
}

/// Calculate swap with exact input amount.
///
/// Standard constant product: `dy = y * dx / (x + dx)` then subtract fees from output.
pub(crate) fn calculate_swap_exact_in(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    fees: &FeeStructure,
) -> crate::Result<(u64, SwapOutput)> {
    let amount_in_128 = amount_in as u128;
    let reserve_in_128 = reserve_in as u128;
    let reserve_out_128 = reserve_out as u128;

    // amount_out_before_fees = (reserve_out * amount_in) / (reserve_in + amount_in)
    let amount_out_before_fees =
        (reserve_out_128 * amount_in_128) / (reserve_in_128 + amount_in_128);

    let (creator_fee, protocol_fee, lp_fee) = fees.calculate_fees(amount_out_before_fees as u64);
    let total_fee = creator_fee + protocol_fee + lp_fee;

    // Refuse rather than wrap. Every fee component rounds up, so a dust trade
    // can owe more in fees than its gross output — and the chain rejects such a
    // trade, so a quote that returns a number here is inventing one. Bare
    // subtraction panics in debug and wraps to near-u64::MAX in release; found
    // by grading 77k live swaps, where the 600-row fixture never reached it.
    let gross_out = amount_out_before_fees as u64;
    let Some(amount_out) = gross_out.checked_sub(total_fee) else {
        return Err(crate::Error::FeesExceedOutput {
            gross_out,
            total_fee,
        });
    };

    // Calculate price impact
    let spot_before = reserve_out_128 as f64 / reserve_in_128 as f64;
    let effective_price = if amount_in > 0 {
        amount_out as f64 / amount_in as f64
    } else {
        0.0
    };
    let price_impact = if spot_before > 0.0 {
        ((1.0 - effective_price / spot_before) * 10000.0) as u16
    } else {
        0
    };

    // Post-swap reserves
    let new_reserve_in = reserve_in_128 + amount_in_128;
    let new_reserve_out = reserve_out_128 - amount_out_before_fees;
    let post_swap_price = if new_reserve_in > 0 {
        new_reserve_out as f64 / new_reserve_in as f64
    } else {
        0.0
    };

    let output = SwapOutput {
        amount_in,
        amount_out,
        fee: total_fee,
        protocol_fee,
        lp_fee: creator_fee + lp_fee, // Group creator + LP fees
        price_impact_bps: price_impact,
        effective_price,
        post_swap_price,
        accounts_used: Vec::new(),
    };

    Ok((amount_out, output))
}

/// Calculate swap with exact output amount (inverse calculation).
///
/// `amount_in = ceil(reserve_in * amount_out / (reserve_out - amount_out))`
pub(crate) fn calculate_swap_exact_out(
    desired_amount_out: u64,
    reserve_in: u64,
    reserve_out: u64,
    fees: &FeeStructure,
) -> SwapOutput {
    let reserve_in_128 = reserve_in as u128;
    let reserve_out_128 = reserve_out as u128;

    // Account for fees on output
    let total_fee_bps = fees.total_bps() as u128;
    // amount_before_fees = ceil(desired_amount_out * 10000 / (10000 - total_fee_bps))
    let amount_before_fees = ceil_div(desired_amount_out as u128 * 10000, 10000 - total_fee_bps);

    if amount_before_fees >= reserve_out_128 {
        warn!(
            desired = desired_amount_out,
            reserve_out = reserve_out,
            "Desired output exceeds pool reserves"
        );
        return SwapOutput::default();
    }

    // amount_in = ceil(reserve_in * amount_before_fees / (reserve_out - amount_before_fees))
    let amount_in = ceil_div(
        reserve_in_128 * amount_before_fees,
        reserve_out_128 - amount_before_fees,
    );

    let (creator_fee, protocol_fee, lp_fee) = fees.calculate_fees(amount_before_fees as u64);
    let total_fee = creator_fee + protocol_fee + lp_fee;

    SwapOutput {
        amount_in: amount_in as u64,
        amount_out: desired_amount_out,
        fee: total_fee,
        protocol_fee,
        lp_fee: creator_fee + lp_fee,
        price_impact_bps: 0, // Would need iterative calc for exact out
        effective_price: if amount_in > 0 {
            desired_amount_out as f64 / amount_in as f64
        } else {
            0.0
        },
        post_swap_price: 0.0,
        accounts_used: Vec::new(),
    }
}

/// Ceiling division.
fn ceil_div(a: u128, b: u128) -> u128 {
    if b == 0 {
        return 0;
    }
    a.div_ceil(b)
}

#[cfg(test)]
mod tests {
    use super::super::state::PumpSwapPool;
    use super::*;
    use crate::traits::SwapParams;

    fn make_pool(base_reserves: u64, quote_reserves: u64) -> PoolWithReserves {
        PoolWithReserves::new(PumpSwapPool::default(), base_reserves, quote_reserves)
    }

    #[test]
    fn constant_product_buy() {
        // Pool: 1M tokens (1e12 smallest), 85 SOL (85e9 lamports)
        let pool = make_pool(1_000_000_000_000, 85_000_000_000);

        let output = pool
            .calculate_swap(&SwapParams::buy(1_000_000_000))
            .unwrap(); // 1 SOL

        // Should get roughly 1M * 1 / (85 + 1) ≈ 11,628 tokens (at 6 decimals)
        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
        assert!(output.amount_in == 1_000_000_000);

        // Sanity: amount_out < total base reserves
        assert!(output.amount_out < 1_000_000_000_000);
    }

    #[test]
    fn constant_product_sell() {
        let pool = make_pool(1_000_000_000_000, 85_000_000_000);

        // Sell 10,000 tokens (10,000 * 1e6 = 1e10)
        let output = pool
            .calculate_swap(&SwapParams::sell(10_000_000_000))
            .unwrap();

        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
        // Should get roughly 85 * 10000/1000000 ≈ 0.85 SOL (minus fees)
        assert!(output.amount_out < 85_000_000_000);
    }

    #[test]
    fn inactive_pool_errors() {
        let pool = PoolWithReserves::new(PumpSwapPool::default(), 0, 0); // No reserves
        let result = pool.calculate_swap(&SwapParams::buy(1_000_000_000));
        assert!(result.is_err());
    }

    #[test]
    fn fee_structure_no_creator() {
        let fees = fee_structure(100.0, false);
        assert_eq!(fees.creator_bps, 0);
        assert_eq!(fees.protocol_bps, 5);
        assert_eq!(fees.lp_bps, 25);
    }

    #[test]
    fn fee_structure_launch_phase() {
        let fees = fee_structure(100.0, true);
        assert_eq!(fees.creator_bps, 30);
        assert_eq!(fees.protocol_bps, 93);
        assert_eq!(fees.lp_bps, 2);
    }

    #[test]
    fn fee_structure_graduated() {
        let fees = fee_structure(1000.0, true);
        assert_eq!(fees.creator_bps, 95);
        assert_eq!(fees.protocol_bps, 5);
        assert_eq!(fees.lp_bps, 20);
    }

    #[test]
    fn fee_structure_mature() {
        let fees = fee_structure(100_000.0, true);
        assert_eq!(fees.creator_bps, 5);
        assert_eq!(fees.protocol_bps, 5);
        assert_eq!(fees.lp_bps, 20);
    }

    #[test]
    fn buy_exact_out() {
        let pool = make_pool(1_000_000_000_000, 85_000_000_000);

        // Want exactly 10,000 tokens
        let output = pool
            .calculate_swap(&SwapParams::buy_exact_out(10_000_000_000))
            .unwrap();

        assert!(output.amount_in > 0);
        assert_eq!(output.amount_out, 10_000_000_000);
    }

    #[test]
    fn conservation_of_value() {
        let pool = make_pool(1_000_000_000_000, 85_000_000_000);

        let buy = pool
            .calculate_swap(&SwapParams::buy(1_000_000_000))
            .unwrap();

        // After buying, if we sell the same tokens, we should get less SOL back (due to fees + spread)
        let pool_after = make_pool(
            1_000_000_000_000 - buy.amount_out,
            85_000_000_000 + buy.amount_in,
        );
        let sell = pool_after
            .calculate_swap(&SwapParams::sell(buy.amount_out))
            .unwrap();

        // Should get back less than we put in (fees + price impact)
        assert!(sell.amount_out < buy.amount_in);
    }

    #[test]
    fn ceil_div_basic() {
        assert_eq!(ceil_div(10, 3), 4);
        assert_eq!(ceil_div(9, 3), 3);
        assert_eq!(ceil_div(0, 5), 0);
        assert_eq!(ceil_div(1, 1), 1);
    }
}
