//! Raydium CPMM swap math.
//!
//! Raydium CPMM uses constant product AMM (x * y = k) formula,
//! identical to Raydium V4 but with Token2022 support and
//! configurable fees via amm_config.

use tracing::{debug, warn};

use crate::events::SwapOutput;
use crate::traits::{SwapDirection, SwapMath, SwapParams};
use crate::{Error, Result};

use super::constants::FEE_RATE_DENOMINATOR;
use super::state::CpmmPoolState;

/// Pool state with current vault reserves for swap calculations.
///
/// CPMM stores reserves in separate vault token accounts, not inline.
/// This struct combines pool config with current reserve balances.
#[derive(Debug, Clone)]
pub struct CpmmPoolWithReserves {
    /// Pool configuration (from on-chain account).
    pub pool: CpmmPoolState,
    /// Current token 0 reserve (from vault, net of fees).
    pub reserve_0: u64,
    /// Current token 1 reserve (from vault, net of fees).
    pub reserve_1: u64,
    /// Fee rate numerator (from amm_config account).
    /// Default: 2500 (0.25% with FEE_RATE_DENOMINATOR = 1_000_000).
    pub trade_fee_rate: u64,
}

impl CpmmPoolWithReserves {
    /// Create pool with reserves and fee rate.
    #[must_use]
    pub fn new(pool: CpmmPoolState, reserve_0: u64, reserve_1: u64, trade_fee_rate: u64) -> Self {
        Self {
            pool,
            reserve_0,
            reserve_1,
            trade_fee_rate,
        }
    }

    /// Create pool with reserves using default fee rate (0.25%).
    #[must_use]
    pub fn with_default_fee(pool: CpmmPoolState, reserve_0: u64, reserve_1: u64) -> Self {
        Self::new(pool, reserve_0, reserve_1, 2500) // 0.25%
    }

    /// Create pool with raw vault balances (auto-deducts accumulated fees).
    pub fn from_vault_balances(
        pool: CpmmPoolState,
        vault_0_balance: u64,
        vault_1_balance: u64,
        trade_fee_rate: u64,
    ) -> Result<Self> {
        let (net_0, net_1) = pool.vault_amount_without_fee(vault_0_balance, vault_1_balance)?;
        Ok(Self::new(pool, net_0, net_1, trade_fee_rate))
    }
}

impl SwapMath for CpmmPoolWithReserves {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        calculate_cpmm_swap(
            &self.pool,
            self.reserve_0,
            self.reserve_1,
            self.trade_fee_rate,
            params,
        )
    }

    fn spot_price(&self) -> f64 {
        self.pool
            .spot_price_with_vaults(self.reserve_0, self.reserve_1)
    }

    fn is_active(&self) -> bool {
        self.pool.is_active()
    }
}

/// Calculate swap output using CPMM constant product formula.
///
/// # Arguments
///
/// * `pool` - Pool state (for decimals and status)
/// * `reserve_0` - Token 0 reserve (net of accumulated fees)
/// * `reserve_1` - Token 1 reserve (net of accumulated fees)
/// * `trade_fee_rate` - Fee rate numerator (denominator is 1_000_000)
/// * `params` - Swap parameters (direction, amount)
///
/// # Formula
///
/// Constant product: x * y = k
/// After fee: amount_in_after_fee = amount_in * (1 - fee_rate / FEE_RATE_DENOMINATOR)
/// Output: dy = y * dx_after_fee / (x + dx_after_fee)
pub fn calculate_cpmm_swap(
    pool: &CpmmPoolState,
    reserve_0: u64,
    reserve_1: u64,
    trade_fee_rate: u64,
    params: &SwapParams,
) -> Result<SwapOutput> {
    let raw_amount = params.amount.amount();
    let amount_in = raw_amount as u128;

    // Calculate fee
    let fee = (amount_in * trade_fee_rate as u128) / FEE_RATE_DENOMINATOR as u128;
    let amount_in_after_fee = amount_in - fee;

    // Determine reserves based on direction
    // Buy = SOL in (token_1 = SOL typically), want token_0
    // Sell = token_0 in, want SOL (token_1)
    let (reserve_in, reserve_out) = match params.direction {
        SwapDirection::Buy => (reserve_1 as u128, reserve_0 as u128),
        SwapDirection::Sell => (reserve_0 as u128, reserve_1 as u128),
    };

    // Constant product: dy = y * dx / (x + dx)
    let denominator = reserve_in + amount_in_after_fee;

    if denominator == 0 {
        warn!(
            target: "raydium_cpmm::swap",
            amount = raw_amount,
            direction = ?params.direction,
            "swap failed: zero denominator (empty reserves)"
        );
        return Err(Error::InsufficientReserves {
            needed: raw_amount,
            available: 0,
        });
    }

    let amount_out = (reserve_out * amount_in_after_fee) / denominator;

    // Calculate new reserves for post-swap price
    let (new_reserve_0, new_reserve_1) = match params.direction {
        SwapDirection::Buy => {
            let new_1 = reserve_1 as u128 + amount_in;
            let new_0 = reserve_0 as u128 - amount_out;
            (new_0, new_1)
        }
        SwapDirection::Sell => {
            let new_0 = reserve_0 as u128 + amount_in;
            let new_1 = reserve_1 as u128 - amount_out;
            (new_0, new_1)
        }
    };

    // Post-swap price (token_0 in terms of token_1)
    let post_swap_price = if new_reserve_0 > 0 {
        let dec_0 = pool.mint_0_decimals as i32;
        let dec_1 = pool.mint_1_decimals as i32;
        let ui_0 = new_reserve_0 as f64 / 10f64.powi(dec_0);
        let ui_1 = new_reserve_1 as f64 / 10f64.powi(dec_1);
        ui_1 / ui_0
    } else {
        0.0
    };

    // Effective price of this swap
    let effective_price = if amount_in > 0 {
        let dec_0 = pool.mint_0_decimals as i32;
        let dec_1 = pool.mint_1_decimals as i32;
        match params.direction {
            SwapDirection::Buy => {
                // SOL (token_1) in, token_0 out
                let ui_in = amount_in as f64 / 10f64.powi(dec_1);
                let ui_out = amount_out as f64 / 10f64.powi(dec_0);
                if ui_out > 0.0 {
                    ui_in / ui_out
                } else {
                    0.0
                }
            }
            SwapDirection::Sell => {
                // token_0 in, SOL (token_1) out
                let ui_in = amount_in as f64 / 10f64.powi(dec_0);
                let ui_out = amount_out as f64 / 10f64.powi(dec_1);
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
    let price_impact_bps = calculate_price_impact_bps(
        reserve_0,
        reserve_1,
        amount_out as u64,
        &params.direction,
        pool,
    );

    debug!(
        target: "raydium_cpmm::swap",
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
        protocol_fee: fee as u64, // CPMM doesn't split fees in the math
        lp_fee: 0,
        price_impact_bps,
        effective_price,
        post_swap_price,
        accounts_used: Vec::new(),
    })
}

/// Calculate price impact of a swap in basis points.
fn calculate_price_impact_bps(
    reserve_0: u64,
    reserve_1: u64,
    amount_out: u64,
    direction: &SwapDirection,
    pool: &CpmmPoolState,
) -> u16 {
    if reserve_0 == 0 || reserve_1 == 0 || amount_out == 0 {
        return 0;
    }

    let dec_0 = pool.mint_0_decimals as i32;
    let dec_1 = pool.mint_1_decimals as i32;

    // Initial price (token_0 in terms of token_1)
    let initial_price =
        (reserve_1 as f64 / 10f64.powi(dec_1)) / (reserve_0 as f64 / 10f64.powi(dec_0));

    // Approximate new reserves after swap
    let (new_0, new_1) = match direction {
        SwapDirection::Buy => {
            // Bought token_0 → reserve_0 decreased
            (reserve_0.saturating_sub(amount_out), reserve_1)
        }
        SwapDirection::Sell => {
            // Sold token_0 → reserve_1 decreased (got token_1 out)
            (reserve_0, reserve_1.saturating_sub(amount_out))
        }
    };

    if new_0 == 0 {
        return 10000; // 100% impact
    }

    let new_price = (new_1 as f64 / 10f64.powi(dec_1)) / (new_0 as f64 / 10f64.powi(dec_0));

    let impact = ((new_price - initial_price) / initial_price).abs();
    (impact * 10000.0).min(10000.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::pubkey::Pubkey;

    fn test_pool() -> CpmmPoolState {
        CpmmPoolState {
            amm_config: Pubkey::new_unique(),
            pool_creator: Pubkey::new_unique(),
            token_0_vault: Pubkey::new_unique(),
            token_1_vault: Pubkey::new_unique(),
            lp_mint: Pubkey::new_unique(),
            token_0_mint: Pubkey::new_unique(),
            token_1_mint: Pubkey::new_unique(),
            token_0_program: Pubkey::new_unique(),
            token_1_program: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            auth_bump: 254,
            status: 0,
            lp_mint_decimals: 9,
            mint_0_decimals: 6, // USDC-like
            mint_1_decimals: 9, // SOL-like
            lp_supply: 1_000_000_000,
            protocol_fees_token_0: 100,
            protocol_fees_token_1: 200,
            fund_fees_token_0: 50,
            fund_fees_token_1: 75,
            open_time: 0,
            recent_epoch: 100,
            creator_fee_on: 0,
            enable_creator_fee: false,
            _padding1: [0u8; 6],
            creator_fees_token_0: 0,
            creator_fees_token_1: 0,
            _padding_reserved: [0u64; 28],
        }
    }

    #[test]
    fn buy_swap_calculation() {
        let pool = test_pool();
        // token_0 = 6 decimals (USDC-like), token_1 = 9 decimals (SOL-like)
        // 1000 tokens (token_0), 1000 SOL (token_1)
        let reserve_0 = 1_000_000_000u64; // 1000 USDC (6 dec)
        let reserve_1 = 1_000_000_000_000u64; // 1000 SOL (9 dec)

        // Buy: SOL in → token_0 out
        let params = SwapParams::buy(100_000_000_000); // 100 SOL
        let output = calculate_cpmm_swap(&pool, reserve_0, reserve_1, 2500, &params).unwrap();

        // After 0.25% fee: 99.75 SOL
        // dy = 1000 USDC * 99.75 / (1000 + 99.75) ≈ 90.7 USDC
        assert!(output.amount_out > 0);
        assert_eq!(output.fee, 250_000_000); // 0.25% of 100 SOL
        assert!(output.amount_out < 100_000_000); // Less than 100 USDC due to price impact
    }

    #[test]
    fn sell_swap_calculation() {
        let pool = test_pool();
        let reserve_0 = 1_000_000_000u64;
        let reserve_1 = 1_000_000_000_000u64;

        // Sell: token_0 in → SOL out
        let params = SwapParams::sell(100_000_000); // 100 USDC
        let output = calculate_cpmm_swap(&pool, reserve_0, reserve_1, 2500, &params).unwrap();

        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
    }

    #[test]
    fn pool_with_reserves_swap_math() {
        let pool = test_pool();
        let reserve_0 = 1_000_000_000u64;
        let reserve_1 = 1_000_000_000_000u64;

        let pool_with_reserves = CpmmPoolWithReserves::with_default_fee(pool, reserve_0, reserve_1);

        assert!(pool_with_reserves.is_active());
        let price = pool_with_reserves.spot_price();
        assert!(price > 0.0);

        let params = SwapParams::buy(100_000_000_000);
        let output = pool_with_reserves.calculate_swap(&params).unwrap();
        assert!(output.amount_out > 0);
    }

    #[test]
    fn from_vault_balances_deducts_fees() {
        let pool = test_pool(); // has protocol_fees_token_0=100, fund_fees=50
        let vault_0 = 10_000u64;
        let vault_1 = 20_000u64;

        let pool_with_reserves =
            CpmmPoolWithReserves::from_vault_balances(pool, vault_0, vault_1, 2500).unwrap();

        assert_eq!(pool_with_reserves.reserve_0, 10_000 - 100 - 50);
        assert_eq!(pool_with_reserves.reserve_1, 20_000 - 200 - 75);
    }

    #[test]
    fn zero_reserves_gives_zero_output() {
        let pool = test_pool();
        let params = SwapParams::buy(1_000_000);

        // With zero reserves, constant product gives 0 output (0 * dx / (0 + dx) = 0)
        let output = calculate_cpmm_swap(&pool, 0, 0, 2500, &params).unwrap();
        assert_eq!(output.amount_out, 0);
    }

    #[test]
    fn swap_disabled_pool_inactive() {
        let mut pool = test_pool();
        pool.status = 1 << 2; // bit 2 = swap disabled

        let pool_with_reserves = CpmmPoolWithReserves::with_default_fee(pool, 1_000_000, 1_000_000);
        assert!(!pool_with_reserves.is_active());
    }

    #[test]
    fn higher_fee_rate_reduces_output() {
        let pool = test_pool();
        let reserve_0 = 1_000_000_000u64;
        let reserve_1 = 1_000_000_000_000u64;
        let params = SwapParams::buy(100_000_000_000);

        let low_fee = calculate_cpmm_swap(&pool, reserve_0, reserve_1, 1000, &params).unwrap();
        let high_fee = calculate_cpmm_swap(&pool, reserve_0, reserve_1, 10000, &params).unwrap();

        assert!(low_fee.amount_out > high_fee.amount_out);
        assert!(low_fee.fee < high_fee.fee);
    }
}
