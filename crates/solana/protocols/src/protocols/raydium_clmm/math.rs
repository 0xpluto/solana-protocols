//! Raydium CLMM swap math — concentrated liquidity with tick crossing.
//!
//! Implements the core Uniswap V3-style concentrated liquidity math:
//! - Q64.64 fixed-point sqrt_price representation
//! - Tick-to-price conversion: `price = 1.0001^tick`
//! - Swap computation with tick crossing and liquidity transitions
//!
//! # Key Formulas
//!
//! For a swap within a single tick range with liquidity `L`:
//! - Buying token0 (token1 in): `delta_0 = L * (1/sqrt_P_new - 1/sqrt_P_old)`
//! - Token1 consumed: `delta_1 = L * (sqrt_P_new - sqrt_P_old)`
//!
//! When crossing a tick boundary, liquidity changes by `liquidity_net`.

use super::constants::{FEE_RATE_DENOMINATOR, MAX_TICK, MIN_TICK, TICK_ARRAY_SIZE};
use super::state::{PoolState, TickArrayState, TickState};

/// Q64.64 scale factor.
const Q64: u128 = 1u128 << 64;

/// Pool state bundled with tick arrays for swap computation.
///
/// CLMM swap math requires tick array data to compute outputs.
/// This struct holds all the data needed for local swap simulation.
#[derive(Debug, Clone)]
pub struct PoolWithTickArrays {
    /// Pool state (sqrt_price, liquidity, tick, fees, etc.).
    pub pool: PoolState,
    /// Tick arrays in the swap direction, ordered from current to target.
    pub tick_arrays: Vec<TickArrayState>,
}

/// Result of a CLMM swap computation.
#[derive(Debug, Clone)]
pub struct ClmmSwapResult {
    /// Amount consumed from input.
    pub amount_in: u64,
    /// Amount produced as output.
    pub amount_out: u64,
    /// Fee charged (in input token).
    pub fee_amount: u64,
    /// New sqrt_price after swap (Q64.64).
    pub sqrt_price_after: u128,
    /// New tick after swap.
    pub tick_after: i32,
    /// Number of tick crossings during swap.
    pub tick_crossings: u32,
}

/// Stateless swap computer — no allocations, pure math.
pub struct ClmmSwapComputer;

impl ClmmSwapComputer {
    /// Compute a swap with exact input amount.
    ///
    /// Walks through tick arrays, crossing tick boundaries and adjusting
    /// liquidity as needed. Returns the amount of output tokens received.
    ///
    /// # Arguments
    ///
    /// * `pool` - Pool state with tick arrays
    /// * `amount_in` - Exact input amount (before fees)
    /// * `is_base_input` - True if input is token0, false if token1
    /// * `fee_rate` - Fee rate (e.g., 2500 = 0.25%)
    pub fn compute_swap(
        pool: &PoolWithTickArrays,
        amount_in: u64,
        is_base_input: bool,
        fee_rate: u64,
    ) -> ClmmSwapResult {
        let mut remaining_in = amount_in as u128;
        let mut total_out: u128 = 0;
        let mut total_fee: u128 = 0;
        let mut sqrt_price = pool.pool.sqrt_price_x64;
        let mut liquidity = pool.pool.liquidity;
        let mut tick = pool.pool.tick_current;
        let mut tick_crossings = 0u32;

        // Collect all initialized ticks from tick arrays, sorted
        let mut initialized_ticks: Vec<&TickState> = pool
            .tick_arrays
            .iter()
            .flat_map(|ta| ta.ticks.iter())
            .filter(|t| t.liquidity_gross > 0)
            .collect();

        // Sort by tick index: ascending for base_input (price decreases),
        // descending for quote_input (price increases)
        if is_base_input {
            // Selling token0 → price goes down → walk ticks downward
            initialized_ticks.sort_by_key(|t| std::cmp::Reverse(t.tick));
            initialized_ticks.retain(|t| t.tick <= tick);
        } else {
            // Buying token0 → price goes up → walk ticks upward
            initialized_ticks.sort_by_key(|t| t.tick);
            initialized_ticks.retain(|t| t.tick > tick);
        }

        // Walk through ticks
        let mut tick_iter = initialized_ticks.iter().peekable();

        while remaining_in > 0 {
            // Determine the next tick boundary
            let next_tick = tick_iter
                .peek()
                .map(|t| t.tick)
                .unwrap_or(if is_base_input { MIN_TICK } else { MAX_TICK });

            let target_sqrt_price = tick_to_sqrt_price_x64(next_tick);

            // Compute how much we can swap within current tick range
            let (consumed, produced, fee, new_sqrt_price, reached_tick) = swap_within_tick_range(
                sqrt_price,
                target_sqrt_price,
                liquidity,
                remaining_in,
                is_base_input,
                fee_rate,
            );

            remaining_in = remaining_in.saturating_sub(consumed + fee);
            total_out += produced;
            total_fee += fee;
            sqrt_price = new_sqrt_price;

            if reached_tick {
                // Cross the tick boundary — adjust liquidity
                if let Some(tick_state) = tick_iter.next() {
                    tick = tick_state.tick;
                    // When moving left (selling), subtract liquidity_net.
                    // When moving right (buying), add liquidity_net.
                    if is_base_input {
                        liquidity = liquidity.wrapping_sub(tick_state.liquidity_net as u128);
                    } else {
                        liquidity = liquidity.wrapping_add(tick_state.liquidity_net as u128);
                    }
                    tick_crossings += 1;
                }
            } else {
                // Didn't reach the tick boundary — swap is complete
                tick = tick_from_sqrt_price_x64(sqrt_price);
                break;
            }

            // Safety: prevent infinite loops
            if tick_crossings > 100 {
                break;
            }
        }

        ClmmSwapResult {
            amount_in: (amount_in as u128 - remaining_in) as u64,
            amount_out: total_out as u64,
            fee_amount: total_fee as u64,
            sqrt_price_after: sqrt_price,
            tick_after: tick,
            tick_crossings,
        }
    }

    /// Estimate output for a swap without tick arrays.
    ///
    /// Uses only the current liquidity and sqrt_price — accurate for
    /// small swaps that don't cross tick boundaries. For larger swaps,
    /// use `compute_swap` with tick arrays.
    pub fn estimate_output(
        sqrt_price_x64: u128,
        liquidity: u128,
        amount_in: u64,
        is_base_input: bool,
        fee_rate: u64,
    ) -> (u64, u64) {
        let amount_after_fee = amount_in as u128 * (FEE_RATE_DENOMINATOR - fee_rate) as u128
            / FEE_RATE_DENOMINATOR as u128;
        let fee = amount_in as u128 - amount_after_fee;

        if liquidity == 0 || sqrt_price_x64 == 0 {
            return (0, fee as u64);
        }

        // Use f64 for the estimate to avoid u128 overflow.
        // This is an estimate — exact math uses compute_swap with tick arrays.
        let l = liquidity as f64;
        let sqrt_p = sqrt_price_x64 as f64 / Q64 as f64;
        let amount = amount_after_fee as f64;

        let output = if is_base_input {
            // Input is token0 → price decreases → output is token1
            // new_sqrt_price = L * sqrt_P / (L + amount * sqrt_P)
            let new_sqrt_p = l * sqrt_p / (l + amount * sqrt_p);
            // delta_1 = L * (sqrt_P_old - sqrt_P_new)
            l * (sqrt_p - new_sqrt_p)
        } else {
            // Input is token1 → price increases → output is token0
            // new_sqrt_price = sqrt_P + amount / L
            let new_sqrt_p = sqrt_p + amount / l;
            // delta_0 = L * (1/sqrt_P_old - 1/sqrt_P_new)
            l * (1.0 / sqrt_p - 1.0 / new_sqrt_p)
        };

        let output = if output < 0.0 { 0.0 } else { output };
        (output as u64, fee as u64)
    }
}

impl PoolWithTickArrays {
    /// Create a new pool with tick arrays.
    #[must_use]
    pub fn new(pool: PoolState, tick_arrays: Vec<TickArrayState>) -> Self {
        Self { pool, tick_arrays }
    }

    /// Get the current price as f64.
    #[must_use]
    pub fn spot_price(&self) -> f64 {
        self.pool.current_price()
    }

    /// Check if the pool is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.pool.is_active()
    }
}

/// Compute swap within a single tick range (no tick crossing).
///
/// Returns: (consumed_in, produced_out, fee, new_sqrt_price, reached_target_tick)
fn swap_within_tick_range(
    sqrt_price: u128,
    target_sqrt_price: u128,
    liquidity: u128,
    amount_in_remaining: u128,
    is_base_input: bool,
    fee_rate: u64,
) -> (u128, u128, u128, u128, bool) {
    if liquidity == 0 {
        return (0, 0, 0, sqrt_price, false);
    }

    // Calculate fee
    let amount_after_fee = amount_in_remaining * (FEE_RATE_DENOMINATOR - fee_rate) as u128
        / FEE_RATE_DENOMINATOR as u128;

    // Calculate max amount that can be swapped to reach target tick
    let (max_in, max_out, new_sqrt_price_at_target) = if is_base_input {
        // Selling token0 → sqrt_price decreases
        if target_sqrt_price >= sqrt_price {
            return (0, 0, 0, sqrt_price, false);
        }
        let max_amount_in = get_delta_amount_0(target_sqrt_price, sqrt_price, liquidity);
        let max_amount_out = get_delta_amount_1(target_sqrt_price, sqrt_price, liquidity);
        (max_amount_in, max_amount_out, target_sqrt_price)
    } else {
        // Buying token0 (token1 input) → sqrt_price increases
        if target_sqrt_price <= sqrt_price {
            return (0, 0, 0, sqrt_price, false);
        }
        let max_amount_in = get_delta_amount_1(sqrt_price, target_sqrt_price, liquidity);
        let max_amount_out = get_delta_amount_0(sqrt_price, target_sqrt_price, liquidity);
        (max_amount_in, max_amount_out, target_sqrt_price)
    };

    if amount_after_fee >= max_in {
        // We can reach the target tick
        let fee = max_in * fee_rate as u128 / (FEE_RATE_DENOMINATOR - fee_rate) as u128;
        (max_in, max_out, fee, new_sqrt_price_at_target, true)
    } else {
        // Partial fill within this tick range
        let (consumed, produced, new_sqrt) = if is_base_input {
            compute_partial_swap_base_input(sqrt_price, liquidity, amount_after_fee)
        } else {
            compute_partial_swap_quote_input(sqrt_price, liquidity, amount_after_fee)
        };
        let fee = amount_in_remaining - amount_after_fee;
        (consumed, produced, fee, new_sqrt, false)
    }
}

/// delta_amount_0 = L * (1/sqrt_P_lower - 1/sqrt_P_upper)
///                = L * Q64 * (sqrt_P_upper - sqrt_P_lower) / (sqrt_P_lower * sqrt_P_upper)
fn get_delta_amount_0(sqrt_price_lower: u128, sqrt_price_upper: u128, liquidity: u128) -> u128 {
    if sqrt_price_lower == 0 || sqrt_price_upper == 0 || liquidity == 0 {
        return 0;
    }
    let diff = sqrt_price_upper.saturating_sub(sqrt_price_lower);
    // Use f64 to avoid u128 overflow on large price ranges
    let l = liquidity as f64;
    let lower = sqrt_price_lower as f64;
    let upper = sqrt_price_upper as f64;
    let d = diff as f64;
    let result = l * d * Q64 as f64 / (lower * upper);
    if result < 0.0 || result > u64::MAX as f64 {
        return u64::MAX as u128;
    }
    result as u128
}

/// delta_amount_1 = L * (sqrt_P_upper - sqrt_P_lower) / Q64
fn get_delta_amount_1(sqrt_price_lower: u128, sqrt_price_upper: u128, liquidity: u128) -> u128 {
    let diff = sqrt_price_upper.saturating_sub(sqrt_price_lower);
    // Use checked mul to avoid overflow
    liquidity
        .checked_mul(diff)
        .map(|v| v / Q64)
        .unwrap_or_else(|| {
            // Fallback to f64 on overflow
            let result = liquidity as f64 * diff as f64 / Q64 as f64;
            result as u128
        })
}

/// Partial swap with token0 input (price decreases).
fn compute_partial_swap_base_input(
    sqrt_price: u128,
    liquidity: u128,
    amount_in: u128,
) -> (u128, u128, u128) {
    if liquidity == 0 || sqrt_price == 0 {
        return (0, 0, sqrt_price);
    }

    // Use f64 to avoid u128 overflow
    let l = liquidity as f64;
    let sqrt_p = sqrt_price as f64;
    let q64 = Q64 as f64;
    let amt = amount_in as f64;

    // new_sqrt_price = L * sqrt_P / (L + amount_in * sqrt_P / Q64)
    let new_sqrt_p = l * sqrt_p / (l + amt * sqrt_p / q64);
    let new_sqrt_price = new_sqrt_p as u128;

    let consumed = amount_in;
    let produced = get_delta_amount_1(new_sqrt_price, sqrt_price, liquidity);

    (consumed, produced, new_sqrt_price)
}

/// Partial swap with token1 input (price increases).
fn compute_partial_swap_quote_input(
    sqrt_price: u128,
    liquidity: u128,
    amount_in: u128,
) -> (u128, u128, u128) {
    if liquidity == 0 {
        return (0, 0, sqrt_price);
    }

    // Use f64 to avoid overflow on delta calculation
    let q64 = Q64 as f64;
    let delta = (amount_in as f64 * q64 / liquidity as f64) as u128;
    let new_sqrt_price = sqrt_price.saturating_add(delta);

    let consumed = amount_in;
    let produced = get_delta_amount_0(sqrt_price, new_sqrt_price, liquidity);

    (consumed, produced, new_sqrt_price)
}

/// Convert tick index to sqrt_price as Q64.64.
///
/// `sqrt_price = 1.0001^(tick/2)` scaled by 2^64.
///
/// Uses f64 arithmetic — sufficient precision for swap math and price limits.
/// For ticks within ±443636, the relative error is negligible.
#[must_use]
pub fn tick_to_sqrt_price_x64(tick: i32) -> u128 {
    if tick == 0 {
        return Q64;
    }
    let sqrt_price = 1.0001_f64.powf(f64::from(tick) / 2.0);
    (sqrt_price * Q64 as f64) as u128
}

/// Convert sqrt_price (Q64.64) to tick index.
///
/// `tick = floor(log(sqrt_price^2) / log(1.0001))`
///       = `floor(2 * log(sqrt_price / 2^64) / log(1.0001))`
#[must_use]
pub fn tick_from_sqrt_price_x64(sqrt_price_x64: u128) -> i32 {
    if sqrt_price_x64 == 0 {
        return MIN_TICK;
    }
    // Use f64 for the logarithm — sufficient precision for tick index
    let sqrt_price = sqrt_price_x64 as f64 / Q64 as f64;
    let price = sqrt_price * sqrt_price;
    let tick = (price.ln() / 1.0001_f64.ln()).floor() as i32;
    tick.clamp(MIN_TICK, MAX_TICK)
}

/// Get the start index of the tick array containing a given tick.
// Public API helper; only exercised by this crate's tests today.
#[allow(dead_code)]
#[must_use]
pub fn tick_array_start_for_tick(tick: i32, tick_spacing: u16) -> i32 {
    let ticks_in_array = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    let mut start = tick / ticks_in_array * ticks_in_array;
    if tick < 0 && tick % ticks_in_array != 0 {
        start -= ticks_in_array;
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::pubkey::Pubkey;

    #[test]
    fn tick_zero_is_price_one() {
        let sqrt_price = tick_to_sqrt_price_x64(0);
        assert_eq!(sqrt_price, Q64, "tick 0 should be exactly Q64");
    }

    #[test]
    fn tick_positive_price_increases() {
        let p0 = tick_to_sqrt_price_x64(0);
        let p1 = tick_to_sqrt_price_x64(100);
        let p2 = tick_to_sqrt_price_x64(10000);
        assert!(p1 > p0, "positive tick should increase sqrt_price");
        assert!(p2 > p1, "larger tick should increase sqrt_price more");
    }

    #[test]
    fn tick_negative_price_decreases() {
        let p0 = tick_to_sqrt_price_x64(0);
        let pn = tick_to_sqrt_price_x64(-100);
        assert!(pn < p0, "negative tick should decrease sqrt_price");
    }

    #[test]
    fn tick_roundtrip_zero() {
        let sqrt = tick_to_sqrt_price_x64(0);
        let tick = tick_from_sqrt_price_x64(sqrt);
        assert_eq!(tick, 0);
    }

    #[test]
    fn tick_roundtrip_positive() {
        for test_tick in [1, 10, 100, 1000, 10000, 100000] {
            let sqrt = tick_to_sqrt_price_x64(test_tick);
            let recovered = tick_from_sqrt_price_x64(sqrt);
            // Allow ±1 tick due to floating point
            assert!(
                (recovered - test_tick).abs() <= 1,
                "tick {test_tick} roundtrip: got {recovered}"
            );
        }
    }

    #[test]
    fn tick_roundtrip_negative() {
        for test_tick in [-1, -10, -100, -1000, -10000, -100000] {
            let sqrt = tick_to_sqrt_price_x64(test_tick);
            let recovered = tick_from_sqrt_price_x64(sqrt);
            assert!(
                (recovered - test_tick).abs() <= 1,
                "tick {test_tick} roundtrip: got {recovered}"
            );
        }
    }

    #[test]
    fn estimate_output_no_liquidity() {
        let (out, _fee) = ClmmSwapComputer::estimate_output(Q64, 0, 1_000_000, true, 2500);
        assert_eq!(out, 0);
    }

    #[test]
    fn estimate_output_basic() {
        // Equal liquidity, price=1.0, small swap
        let liquidity = 1_000_000_000_000u128; // 1T liquidity
        let sqrt_price = Q64; // price = 1.0

        let (out, fee) =
            ClmmSwapComputer::estimate_output(sqrt_price, liquidity, 1_000_000, true, 2500);

        assert!(out > 0, "should produce output");
        assert!(fee > 0, "should charge fee");
        assert!(out < 1_000_000, "output should be less than input (fees)");
    }

    #[test]
    fn estimate_output_buy() {
        let liquidity = 1_000_000_000_000u128;
        let sqrt_price = Q64;

        let (out, fee) =
            ClmmSwapComputer::estimate_output(sqrt_price, liquidity, 1_000_000, false, 2500);

        assert!(out > 0, "buy should produce token0");
        assert!(fee > 0, "should charge fee");
    }

    #[test]
    fn delta_amount_0_basic() {
        let lower = Q64; // price = 1.0
        let upper = Q64 + Q64 / 100; // price ≈ 1.01
        let liquidity = 1_000_000_000_000u128;

        let delta = get_delta_amount_0(lower, upper, liquidity);
        assert!(delta > 0, "should have positive delta_0");
    }

    #[test]
    fn delta_amount_1_basic() {
        let lower = Q64;
        let upper = Q64 + Q64 / 100;
        let liquidity = 1_000_000_000_000u128;

        let delta = get_delta_amount_1(lower, upper, liquidity);
        assert!(delta > 0, "should have positive delta_1");
    }

    #[test]
    fn compute_swap_no_tick_arrays_still_works() {
        let pool = PoolState {
            sqrt_price_x64: Q64,
            liquidity: 1_000_000_000_000,
            tick_current: 0,
            tick_spacing: 10,
            ..PoolState::default()
        };

        let pwt = PoolWithTickArrays::new(pool, vec![]);

        let result = ClmmSwapComputer::compute_swap(&pwt, 1_000_000, true, 2500);

        // Even without tick arrays, should produce some output
        // (uses current liquidity until it would cross first tick)
        assert!(result.amount_out > 0 || result.amount_in == 0);
        assert_eq!(result.tick_crossings, 0);
    }

    #[test]
    fn compute_swap_with_tick_array() {
        let pool = PoolState {
            sqrt_price_x64: Q64,
            liquidity: 1_000_000_000_000,
            tick_current: 0,
            tick_spacing: 1,
            ..PoolState::default()
        };

        // Create a tick array with some initialized ticks
        let mut ticks = [TickState::default(); 60];
        ticks[30] = TickState {
            tick: -30,
            liquidity_net: 500_000_000_000,
            liquidity_gross: 500_000_000_000,
            ..TickState::default()
        };

        let tick_array = TickArrayState {
            pool_id: Pubkey::default(),
            start_tick_index: -60,
            ticks,
            initialized_tick_count: 1,
            recent_epoch: 0,
        };

        let pwt = PoolWithTickArrays::new(pool, vec![tick_array]);

        let result = ClmmSwapComputer::compute_swap(&pwt, 1_000_000, true, 2500);

        assert!(result.amount_out > 0);
        assert!(result.fee_amount > 0);
    }

    #[test]
    fn tick_array_start_computation() {
        assert_eq!(tick_array_start_for_tick(0, 1), 0);
        assert_eq!(tick_array_start_for_tick(59, 1), 0);
        assert_eq!(tick_array_start_for_tick(60, 1), 60);
        assert_eq!(tick_array_start_for_tick(-1, 1), -60);
    }
}
