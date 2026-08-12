//! Raydium V4 swap math.
//!
//! Raydium V4 uses constant product AMM (x * y = k) formula.

use tracing::{debug, warn};

use crate::events::SwapOutput;
use crate::traits::{SwapDirection, SwapMath, SwapParams};
use crate::{Error, Result};

use super::LiquidityPool;

/// Pool state with current reserves for swap calculations.
///
/// Raydium V4 stores reserves in separate vault token accounts,
/// not in the pool state itself. This struct combines pool config
/// with current reserve balances.
#[derive(Debug, Clone)]
pub struct PoolWithReserves {
    /// Pool configuration.
    pub pool: LiquidityPool,
    /// Current base token reserve (from vault).
    pub base_reserve: u64,
    /// Current quote token reserve (from vault).
    pub quote_reserve: u64,
}

impl PoolWithReserves {
    /// Create pool with reserves.
    #[must_use]
    pub fn new(pool: LiquidityPool, base_reserve: u64, quote_reserve: u64) -> Self {
        Self {
            pool,
            base_reserve,
            quote_reserve,
        }
    }
}

impl SwapMath for PoolWithReserves {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        calculate_swap_output(&self.pool, self.base_reserve, self.quote_reserve, params)
    }

    fn spot_price(&self) -> f64 {
        self.pool
            .calculate_spot_price(self.base_reserve, self.quote_reserve)
    }

    fn is_active(&self) -> bool {
        self.pool.is_active()
    }
}

/// Calculate swap output using constant product formula.
///
/// # Arguments
///
/// * `pool` - Pool configuration (for fees and decimals)
/// * `base_reserve` - Current base token reserve
/// * `quote_reserve` - Current quote token reserve
/// * `params` - Swap parameters
///
/// # Formula
///
/// For constant product AMM: x * y = k
///
/// When swapping dx of X for dy of Y:
/// - After swap: (x + dx) * (y - dy) = k
/// - dy = y - k / (x + dx)
/// - dy = y - (x * y) / (x + dx)
/// - dy = y * dx / (x + dx)
pub fn calculate_swap_output(
    pool: &LiquidityPool,
    base_reserve: u64,
    quote_reserve: u64,
    params: &SwapParams,
) -> Result<SwapOutput> {
    // Get fee rate
    let fee_numerator = pool.trade_fee_numerator as u128;
    let fee_denominator = pool.trade_fee_denominator as u128;

    // Extract raw amount from SwapAmount
    let raw_amount = params.amount.amount();
    let amount_in = raw_amount as u128;

    // Calculate fee
    let fee = (amount_in * fee_numerator)
        .checked_div(fee_denominator)
        .unwrap_or(0);

    let amount_in_after_fee = amount_in - fee;

    // Determine reserves based on direction
    let (reserve_in, reserve_out) = match params.direction {
        SwapDirection::Buy => {
            // Buying base with quote (quote -> base)
            (quote_reserve as u128, base_reserve as u128)
        }
        SwapDirection::Sell => {
            // Selling base for quote (base -> quote)
            (base_reserve as u128, quote_reserve as u128)
        }
    };

    // Constant product: dy = y * dx / (x + dx)
    let numerator = reserve_out * amount_in_after_fee;
    let denominator = reserve_in + amount_in_after_fee;

    if denominator == 0 {
        warn!(
            target: "raydium_v4::swap",
            amount = raw_amount,
            direction = ?params.direction,
            "swap failed: zero denominator (empty reserves)"
        );
        return Err(Error::InsufficientReserves {
            needed: raw_amount,
            available: 0,
        });
    }

    let amount_out = numerator / denominator;

    // Calculate new reserves
    let (new_base, new_quote) = match params.direction {
        SwapDirection::Buy => {
            let new_quote = quote_reserve as u128 + amount_in;
            let new_base = base_reserve as u128 - amount_out;
            (new_base, new_quote)
        }
        SwapDirection::Sell => {
            let new_base = base_reserve as u128 + amount_in;
            let new_quote = quote_reserve as u128 - amount_out;
            (new_base, new_quote)
        }
    };

    // Calculate new price (quote per base)
    let post_swap_price = if new_base > 0 {
        let base_decimal = pool.base_decimal;
        let quote_decimal = pool.quote_decimal;
        let base_ui = new_base as f64 / 10f64.powi(base_decimal as i32);
        let quote_ui = new_quote as f64 / 10f64.powi(quote_decimal as i32);
        quote_ui / base_ui
    } else {
        0.0
    };

    // Calculate effective price
    let effective_price = if amount_in > 0 {
        let base_decimal = pool.base_decimal;
        let quote_decimal = pool.quote_decimal;
        match params.direction {
            SwapDirection::Buy => {
                // Quote in, base out: price = quote_amount / base_amount
                let quote_ui = amount_in as f64 / 10f64.powi(quote_decimal as i32);
                let base_ui = amount_out as f64 / 10f64.powi(base_decimal as i32);
                quote_ui / base_ui
            }
            SwapDirection::Sell => {
                // Base in, quote out: price = quote_amount / base_amount
                let base_ui = amount_in as f64 / 10f64.powi(base_decimal as i32);
                let quote_ui = amount_out as f64 / 10f64.powi(quote_decimal as i32);
                quote_ui / base_ui
            }
        }
    } else {
        0.0
    };

    // Calculate price impact in bps
    let price_impact_bps = calculate_price_impact_bps(
        base_reserve,
        quote_reserve,
        amount_out as u64,
        &params.direction,
        pool,
    );

    debug!(
        target: "raydium_v4::swap",
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
        protocol_fee: fee as u64, // Raydium V4 doesn't split fees
        lp_fee: 0,
        price_impact_bps,
        effective_price,
        post_swap_price,
        accounts_used: Vec::new(),
    })
}

/// Calculate price impact of a swap in basis points.
fn calculate_price_impact_bps(
    base_reserve: u64,
    quote_reserve: u64,
    amount_out: u64,
    direction: &SwapDirection,
    pool: &LiquidityPool,
) -> u16 {
    if base_reserve == 0 || quote_reserve == 0 || amount_out == 0 {
        return 0;
    }

    let base_decimal = pool.base_decimal;
    let quote_decimal = pool.quote_decimal;

    // Initial price (quote per base)
    let initial_price = (quote_reserve as f64 / 10f64.powi(quote_decimal as i32))
        / (base_reserve as f64 / 10f64.powi(base_decimal as i32));

    // Calculate new reserves
    let (new_base, new_quote) = match direction {
        SwapDirection::Buy => {
            // Bought base, so base reserve decreased
            (base_reserve.saturating_sub(amount_out), quote_reserve)
        }
        SwapDirection::Sell => {
            // Sold base, so base reserve increased
            (base_reserve, quote_reserve.saturating_sub(amount_out))
        }
    };

    if new_base == 0 {
        return 10000; // 100% price impact
    }

    // Approximate new price
    let new_price = (new_quote as f64 / 10f64.powi(quote_decimal as i32))
        / (new_base as f64 / 10f64.powi(base_decimal as i32));

    // Price impact = (new_price - initial_price) / initial_price
    let impact = ((new_price - initial_price) / initial_price).abs();

    // Convert to basis points (1 bp = 0.01%)
    (impact * 10000.0).min(10000.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> LiquidityPool {
        LiquidityPool {
            status: 3,
            nonce: 255,
            max_order: 7,
            depth: 3,
            base_decimal: 9,  // 9 decimals (like SOL)
            quote_decimal: 6, // 6 decimals (like USDC)
            state: 0,
            reset_flag: 0,
            min_size: 0,
            vol_max_cut_ratio: 500,
            amount_wave_ratio: 50000000,
            base_lot_size: 1000000,
            quote_lot_size: 1000,
            min_price_multiplier: 1,
            max_price_multiplier: 1000000000,
            system_decimal_value: 1000000000,
            min_separate_numerator: 5,
            min_separate_denominator: 10000,
            trade_fee_numerator: 25, // 0.25% fee
            trade_fee_denominator: 10000,
            pnl_numerator: 12,
            pnl_denominator: 100,
            swap_fee_numerator: 25,
            swap_fee_denominator: 10000,
            base_need_take_pnl: 0,
            quote_need_take_pnl: 0,
            quote_total_pnl: 0,
            base_total_pnl: 0,
            quote_total_deposited: 0,
            base_total_deposited: 0,
            swap_base_in_amount: 0,
            swap_quote_out_amount: 0,
            swap_base2_quote_fee: 0,
            swap_quote_in_amount: 0,
            swap_base_out_amount: 0,
            swap_quote2_base_fee: 0,
            base_vault: solana_program::pubkey::Pubkey::new_unique(),
            quote_vault: solana_program::pubkey::Pubkey::new_unique(),
            base_mint: solana_program::pubkey::Pubkey::new_unique(),
            quote_mint: solana_program::pubkey::Pubkey::new_unique(),
            lp_mint: solana_program::pubkey::Pubkey::new_unique(),
            open_orders: solana_program::pubkey::Pubkey::new_unique(),
            market_id: solana_program::pubkey::Pubkey::new_unique(),
            market_program_id: solana_program::pubkey::Pubkey::new_unique(),
            target_orders: solana_program::pubkey::Pubkey::new_unique(),
            withdraw_queue: solana_program::pubkey::Pubkey::new_unique(),
            lp_vault: solana_program::pubkey::Pubkey::new_unique(),
            owner: solana_program::pubkey::Pubkey::new_unique(),
            pnl_owner: solana_program::pubkey::Pubkey::new_unique(),
        }
    }

    #[test]
    fn buy_swap_calculation() {
        let pool = test_pool();
        // 1000 tokens (base), 1000 USDC (quote) -> price = 1 USDC per token
        let base_reserve = 1_000_000_000_000u64; // 1000 tokens (9 decimals)
        let quote_reserve = 1_000_000_000u64; // 1000 USDC (6 decimals)

        // Buy 100 USDC worth of tokens
        let params = SwapParams::buy(100_000_000); // 100 USDC
        let output = calculate_swap_output(&pool, base_reserve, quote_reserve, &params).unwrap();

        // After 0.25% fee: 99.75 USDC in
        // dy = y * dx / (x + dx) = 1000 * 99.75 / (1000 + 99.75) ≈ 90.7 tokens
        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
        assert_eq!(output.fee, 250_000); // 0.25% of 100 USDC
    }

    #[test]
    fn sell_swap_calculation() {
        let pool = test_pool();
        let base_reserve = 1_000_000_000_000u64; // 1000 tokens
        let quote_reserve = 1_000_000_000u64; // 1000 USDC

        // Sell 100 tokens
        let params = SwapParams::sell(100_000_000_000); // 100 tokens
        let output = calculate_swap_output(&pool, base_reserve, quote_reserve, &params).unwrap();

        // After 0.25% fee: 99.75 tokens in
        // dy = 1000 USDC * 99.75 / (1000 + 99.75) ≈ 90.7 USDC
        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
    }

    #[test]
    fn pool_with_reserves_swap_math() {
        let pool = test_pool();
        let base_reserve = 1_000_000_000_000u64;
        let quote_reserve = 1_000_000_000u64;

        let pool_with_reserves = PoolWithReserves::new(pool, base_reserve, quote_reserve);

        assert!(pool_with_reserves.is_active());
        let price = pool_with_reserves.spot_price();
        assert!((price - 1.0).abs() < 0.001); // 1 USDC per token

        let params = SwapParams::buy(100_000_000);
        let output = pool_with_reserves.calculate_swap(&params).unwrap();
        assert!(output.amount_out > 0);
    }
}
