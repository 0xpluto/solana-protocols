//! Raydium Launchpad swap math.
//!
//! Raydium Launchpad uses a bonding curve (constant product variant)
//! with virtual reserves. The formula is identical to Pumpfun's bonding
//! curve: price = virtual_quote / virtual_base, swap via x*y=k.

use tracing::{debug, warn};

use crate::events::SwapOutput;
use crate::traits::{SwapDirection, SwapMath, SwapParams};
use crate::{Error, Result};

use super::constants::FEE_RATE_DENOMINATOR;
use super::state::LaunchpadPoolState;

/// Launchpad pool ready for swap calculations.
///
/// Wraps the pool state with the trade fee rate (from the platform/global config).
#[derive(Debug, Clone)]
pub struct LaunchpadSwapPool {
    /// Pool state.
    pub pool: LaunchpadPoolState,
    /// Trade fee rate numerator (denominator is 1_000_000).
    /// Typical: 10_000 = 1%.
    pub trade_fee_rate: u64,
}

impl LaunchpadSwapPool {
    /// Create swap pool with explicit fee rate.
    #[must_use]
    pub fn new(pool: LaunchpadPoolState, trade_fee_rate: u64) -> Self {
        Self {
            pool,
            trade_fee_rate,
        }
    }

    /// Create swap pool with default 1% fee.
    #[must_use]
    pub fn with_default_fee(pool: LaunchpadPoolState) -> Self {
        Self::new(pool, 10_000) // 1%
    }
}

impl SwapMath for LaunchpadSwapPool {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        calculate_launchpad_swap(&self.pool, self.trade_fee_rate, params)
    }

    fn spot_price(&self) -> f64 {
        self.pool.spot_price()
    }

    fn is_active(&self) -> bool {
        self.pool.is_active()
    }
}

/// Calculate swap output using Launchpad bonding curve.
///
/// Uses constant product formula with virtual reserves:
/// - base_reserve = virtual_base - real_base
/// - quote_reserve = virtual_quote + real_quote
/// - k = base_reserve * quote_reserve
///
/// Buy: quote in → base out
/// Sell: base in → quote out
pub fn calculate_launchpad_swap(
    pool: &LaunchpadPoolState,
    trade_fee_rate: u64,
    params: &SwapParams,
) -> Result<SwapOutput> {
    let raw_amount = params.amount.amount();
    let amount_in = raw_amount as u128;

    // Calculate fee
    let fee = (amount_in * trade_fee_rate as u128) / FEE_RATE_DENOMINATOR as u128;
    let amount_in_after_fee = amount_in - fee;

    // Get effective reserves
    let base_reserve = pool.base_reserve() as u128;
    let quote_reserve = pool.quote_reserve() as u128;

    // Determine input/output reserves
    let (reserve_in, reserve_out) = match params.direction {
        SwapDirection::Buy => (quote_reserve, base_reserve), // quote in → base out
        SwapDirection::Sell => (base_reserve, quote_reserve), // base in → quote out
    };

    // Constant product: dy = y * dx / (x + dx)
    let denominator = reserve_in + amount_in_after_fee;

    if denominator == 0 {
        warn!(
            target: "raydium_launchpad::swap",
            amount = raw_amount,
            direction = ?params.direction,
            "swap failed: zero denominator"
        );
        return Err(Error::InsufficientReserves {
            needed: raw_amount,
            available: 0,
        });
    }

    let amount_out = (reserve_out * amount_in_after_fee) / denominator;

    // Calculate post-swap reserves
    let (new_base, new_quote) = match params.direction {
        SwapDirection::Buy => (base_reserve - amount_out, quote_reserve + amount_in),
        SwapDirection::Sell => (base_reserve + amount_in, quote_reserve - amount_out),
    };

    // Post-swap price (quote per base, decimal-adjusted)
    let post_swap_price = if new_base > 0 {
        let dec_adj = 10f64.powi(pool.base_decimals as i32 - pool.quote_decimals as i32);
        (new_quote as f64 / new_base as f64) * dec_adj
    } else {
        0.0
    };

    // Effective price of this swap
    let effective_price = if amount_in > 0 {
        match params.direction {
            SwapDirection::Buy => {
                let ui_in = amount_in as f64 / 10f64.powi(pool.quote_decimals as i32);
                let ui_out = amount_out as f64 / 10f64.powi(pool.base_decimals as i32);
                if ui_out > 0.0 {
                    ui_in / ui_out
                } else {
                    0.0
                }
            }
            SwapDirection::Sell => {
                let ui_in = amount_in as f64 / 10f64.powi(pool.base_decimals as i32);
                let ui_out = amount_out as f64 / 10f64.powi(pool.quote_decimals as i32);
                if ui_in > 0.0 {
                    ui_out / ui_in
                } else {
                    0.0
                }
            }
        }
    } else {
        0.0
    };

    // Price impact
    let initial_price = pool.spot_price();
    let price_impact_bps = if initial_price > 0.0 && post_swap_price > 0.0 {
        let impact = ((post_swap_price - initial_price) / initial_price).abs();
        (impact * 10000.0).min(10000.0) as u16
    } else {
        0
    };

    debug!(
        target: "raydium_launchpad::swap",
        amount_in = raw_amount,
        amount_out = amount_out as u64,
        fee = fee as u64,
        price_impact_bps,
        direction = ?params.direction,
        "swap calculated"
    );

    Ok(SwapOutput {
        amount_in: raw_amount,
        amount_out: amount_out as u64,
        fee: fee as u64,
        protocol_fee: fee as u64,
        lp_fee: 0,
        price_impact_bps,
        effective_price,
        post_swap_price,
        accounts_used: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::state::VestingSchedule;
    use super::*;
    use solana_program::pubkey::Pubkey;

    fn test_pool() -> LaunchpadPoolState {
        LaunchpadPoolState {
            epoch: 1,
            auth_bump: 255,
            status: 1, // Live
            base_decimals: 6,
            quote_decimals: 9,
            migrate_type: 0,
            supply: 1_000_000_000_000,
            total_base_sell: 0,
            virtual_base: 1_000_000_000_000_000, // 1B base tokens (6 dec)
            virtual_quote: 30_000_000_000,       // 30 SOL (9 dec)
            real_base: 200_000_000_000_000,      // 200M tokens sold
            real_quote: 10_000_000_000,          // 10 SOL collected
            total_quote_fund_raising: 0,
            quote_protocol_fee: 0,
            platform_fee: 0,
            migrate_fee: 0,
            vesting_schedule: VestingSchedule {
                total_locked_amount: 0,
                cliff_period: 0,
                unlock_period: 0,
                start_time: 0,
                allocated_share_amount: 0,
            },
            global_config: Pubkey::new_unique(),
            platform_config: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            creator: Pubkey::new_unique(),
            token_program_flag: 0,
            amm_creator_fee_on: 0,
        }
    }

    #[test]
    fn buy_swap() {
        let pool = test_pool();
        // base_reserve = 1e15 - 2e14 = 8e14
        // quote_reserve = 3e10 + 1e10 = 4e10
        let params = SwapParams::buy(1_000_000_000); // 1 SOL

        let output = calculate_launchpad_swap(&pool, 10_000, &params).unwrap();
        assert!(output.amount_out > 0);
        // 1% fee on 1 SOL = 0.01 SOL = 10_000_000 lamports
        assert_eq!(output.fee, 10_000_000);
    }

    #[test]
    fn sell_swap() {
        let pool = test_pool();
        let params = SwapParams::sell(100_000_000_000); // 100K tokens (6 dec)

        let output = calculate_launchpad_swap(&pool, 10_000, &params).unwrap();
        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
    }

    #[test]
    fn swap_pool_trait() {
        let pool = test_pool();
        let swap_pool = LaunchpadSwapPool::with_default_fee(pool);

        assert!(swap_pool.is_active());
        assert!(swap_pool.spot_price() > 0.0);

        let params = SwapParams::buy(1_000_000_000);
        let output = swap_pool.calculate_swap(&params).unwrap();
        assert!(output.amount_out > 0);
    }

    #[test]
    fn inactive_pool() {
        let mut pool = test_pool();
        pool.status = 0; // Fund phase
        let swap_pool = LaunchpadSwapPool::with_default_fee(pool);
        assert!(!swap_pool.is_active());
    }

    #[test]
    fn higher_fee_reduces_output() {
        let pool = test_pool();
        let params = SwapParams::buy(1_000_000_000);

        let low_fee = calculate_launchpad_swap(&pool, 5_000, &params).unwrap();
        let high_fee = calculate_launchpad_swap(&pool, 50_000, &params).unwrap();

        assert!(low_fee.amount_out > high_fee.amount_out);
    }
}
