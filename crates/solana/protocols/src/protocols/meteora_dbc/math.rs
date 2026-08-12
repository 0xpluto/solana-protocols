//! Meteora DBC swap math.
//!
//! DBC uses a bonding curve with `sqrt_price` (Q64 fixed-point).
//! Reserves are stored inline in VirtualPool, so no external vault fetching needed.
//!
//! The curve behaves like constant product (x * y = k) with the inline reserves.

use tracing::debug;

use crate::events::SwapOutput;
use crate::traits::{SwapDirection, SwapMath, SwapParams};
use crate::{Error, Result};

use super::constants::FEE_DENOMINATOR;
use super::state::VirtualPool;

/// DBC pool ready for swap calculation.
///
/// Reserves are inline in VirtualPool state (no external vault fetch needed).
#[derive(Debug, Clone)]
pub struct DbcSwapPool {
    /// Pool state with inline reserves.
    pub pool: VirtualPool,
    /// Trade fee rate (out of FEE_DENOMINATOR = 1_000_000_000).
    /// Default: 10_000_000 = 1%.
    pub trade_fee_rate: u64,
}

/// Default fee rate: 1% (10_000_000 / 1_000_000_000).
pub const DEFAULT_FEE_RATE: u64 = 10_000_000;

impl DbcSwapPool {
    /// Create swap pool with custom fee rate.
    #[must_use]
    pub fn new(pool: VirtualPool, trade_fee_rate: u64) -> Self {
        Self {
            pool,
            trade_fee_rate,
        }
    }

    /// Create swap pool with default 1% fee.
    #[must_use]
    pub fn with_default_fee(pool: VirtualPool) -> Self {
        Self::new(pool, DEFAULT_FEE_RATE)
    }
}

impl SwapMath for DbcSwapPool {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        calculate_dbc_swap(&self.pool, self.trade_fee_rate, params)
    }

    fn spot_price(&self) -> f64 {
        self.pool.spot_price()
    }

    fn is_active(&self) -> bool {
        self.pool.is_active()
    }
}

/// Calculate DBC swap using constant product with inline reserves.
///
/// Buy: quote (SOL) in → base (token) out.
/// Sell: base (token) in → quote (SOL) out.
///
/// Formula: amount_out = reserve_out * amount_in_after_fee / (reserve_in + amount_in_after_fee)
pub fn calculate_dbc_swap(
    pool: &VirtualPool,
    trade_fee_rate: u64,
    params: &SwapParams,
) -> Result<SwapOutput> {
    if !pool.is_active() {
        return Err(Error::protocol_error(
            "MeteoraDbC",
            "pool is migrated/inactive",
        ));
    }

    let raw_amount = params.amount.amount();

    // Fee calculation
    let fee = raw_amount
        .checked_mul(trade_fee_rate)
        .and_then(|v| v.checked_div(FEE_DENOMINATOR))
        .unwrap_or(0);
    let amount_after_fee = raw_amount.saturating_sub(fee);

    // Determine reserves based on direction
    let (reserve_in, reserve_out) = match params.direction {
        SwapDirection::Buy => {
            // Quote (SOL) in → Base (token) out
            (pool.quote_reserve, pool.base_reserve)
        }
        SwapDirection::Sell => {
            // Base (token) in → Quote (SOL) out
            (pool.base_reserve, pool.quote_reserve)
        }
    };

    // Constant product: amount_out = reserve_out * amount_in / (reserve_in + amount_in)
    let denominator = reserve_in.saturating_add(amount_after_fee);
    if denominator == 0 {
        return Ok(SwapOutput {
            amount_in: raw_amount,
            amount_out: 0,
            fee,
            protocol_fee: fee / 2,
            lp_fee: fee / 2,
            price_impact_bps: 0,
            effective_price: 0.0,
            post_swap_price: pool.spot_price(),
            accounts_used: Vec::new(),
        });
    }

    let amount_out = (reserve_out as u128)
        .checked_mul(amount_after_fee as u128)
        .and_then(|v| v.checked_div(denominator as u128))
        .unwrap_or(0) as u64;

    // Check reserves
    if amount_out > reserve_out {
        return Err(Error::InsufficientReserves {
            needed: amount_out,
            available: reserve_out,
        });
    }

    // Price impact
    let price_impact_bps = if reserve_out > 0 {
        let fraction = amount_out as f64 / reserve_out as f64;
        (fraction * 10000.0).min(10000.0) as u16
    } else {
        10000
    };

    // Effective price
    let effective_price = if raw_amount > 0 && amount_out > 0 {
        amount_out as f64 / raw_amount as f64
    } else {
        0.0
    };

    // Post-swap price from new reserves
    let new_reserve_in = reserve_in.saturating_add(amount_after_fee);
    let new_reserve_out = reserve_out.saturating_sub(amount_out);
    let post_swap_price = if new_reserve_out > 0 {
        new_reserve_in as f64 / new_reserve_out as f64
    } else {
        0.0
    };

    debug!(
        target: "meteora_dbc::swap",
        amount_in = raw_amount,
        amount_out,
        fee,
        price_impact_bps,
        direction = ?params.direction,
        "swap calculated"
    );

    Ok(SwapOutput {
        amount_in: raw_amount,
        amount_out,
        fee,
        protocol_fee: fee / 2,
        lp_fee: fee / 2,
        price_impact_bps,
        effective_price,
        post_swap_price,
        accounts_used: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::state::*;
    use super::*;
    use solana_program::pubkey::Pubkey;

    fn test_pool() -> VirtualPool {
        VirtualPool {
            volatility_tracker: VolatilityTracker::default(),
            config: Pubkey::new_unique(),
            creator: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            base_reserve: 800_000_000_000_000, // 800M tokens
            quote_reserve: 30_000_000_000,     // 30 SOL
            protocol_base_fee: 0,
            protocol_quote_fee: 0,
            partner_base_fee: 0,
            partner_quote_fee: 0,
            sqrt_price: 1u128 << 64, // price = 1.0
            activation_point: 0,
            pool_type: 0,
            is_migrated: 0,
            is_partner_withdraw_surplus: 0,
            is_protocol_withdraw_surplus: 0,
            migration_progress: 0,
            is_withdraw_leftover: 0,
            is_creator_withdraw_surplus: 0,
            migration_fee_withdraw_status: 0,
            metrics: PoolMetrics::default(),
            finish_curve_timestamp: 0,
            creator_base_fee: 0,
            creator_quote_fee: 0,
            legacy_creation_fee_bits: 0,
            creation_fee_bits: 0,
            _padding_0: [0u8; 6],
            _padding_1: [0u64; 6],
        }
    }

    #[test]
    fn buy_estimate() {
        let pool = test_pool();
        let params = SwapParams::buy(1_000_000_000); // 1 SOL

        let output = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE, &params).unwrap();
        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
        assert_eq!(output.amount_in, 1_000_000_000);
    }

    #[test]
    fn sell_estimate() {
        let pool = test_pool();
        let params = SwapParams::sell(100_000_000_000); // 100K tokens

        let output = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE, &params).unwrap();
        assert!(output.amount_out > 0);
    }

    #[test]
    fn fee_deducted() {
        let pool = test_pool();
        let params = SwapParams::buy(1_000_000_000);

        let output = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE, &params).unwrap();
        // 1% of 1 SOL = 10_000_000
        assert_eq!(output.fee, 10_000_000);
    }

    #[test]
    fn swap_pool_trait() {
        let pool = test_pool();
        let swap_pool = DbcSwapPool::with_default_fee(pool);

        assert!(swap_pool.is_active());
        let price = swap_pool.spot_price();
        assert!((price - 1.0).abs() < 0.001);

        let params = SwapParams::buy(1_000_000_000);
        let output = swap_pool.calculate_swap(&params).unwrap();
        assert!(output.amount_out > 0);
    }

    #[test]
    fn inactive_pool_errors() {
        let mut pool = test_pool();
        pool.is_migrated = 1;

        let params = SwapParams::buy(1_000_000_000);
        let result = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE, &params);
        assert!(result.is_err());
    }

    #[test]
    fn higher_fee_less_output() {
        let pool = test_pool();
        let params = SwapParams::buy(1_000_000_000);

        let low_fee = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE, &params).unwrap();
        let high_fee = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE * 5, &params).unwrap();

        assert!(low_fee.amount_out > high_fee.amount_out);
    }

    #[test]
    fn zero_reserves_gives_zero_output() {
        let mut pool = test_pool();
        pool.base_reserve = 0;
        pool.quote_reserve = 0;

        let params = SwapParams::buy(1_000_000_000);
        let output = calculate_dbc_swap(&pool, DEFAULT_FEE_RATE, &params).unwrap();
        assert_eq!(output.amount_out, 0);
    }
}
