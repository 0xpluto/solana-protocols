//! Raydium V4 pool account state — **compilation stub**.
//!
//! The exhaustive account layout is non-trivial and not required for the
//! work currently on this branch. This module provides the `LiquidityPool`
//! struct with the full public field surface other modules already depend
//! on (from `math.rs`, `accounts.rs`, and the unified `PoolState` enum),
//! plus minimal helper methods — enough for the crate to compile.
//!
//! `from_account_data` returns an error; any code path that actually
//! parses Raydium V4 pools will surface a clear runtime failure until the
//! real layout is filled in.

use solana_program::pubkey::Pubkey;

use crate::error::{Error, Result};

/// Raydium V4 AMM pool state.
///
/// Field list mirrors the on-chain layout used by downstream code; values
/// are populated only when a real `from_account_data` implementation is
/// added.
#[derive(Debug, Clone)]
pub struct LiquidityPool {
    pub status: u64,
    pub nonce: u64,
    pub max_order: u64,
    pub depth: u64,
    pub base_decimal: u64,
    pub quote_decimal: u64,
    pub state: u64,
    pub reset_flag: u64,
    pub min_size: u64,
    pub vol_max_cut_ratio: u64,
    pub amount_wave_ratio: u64,
    pub base_lot_size: u64,
    pub quote_lot_size: u64,
    pub min_price_multiplier: u64,
    pub max_price_multiplier: u64,
    pub system_decimal_value: u64,
    pub min_separate_numerator: u64,
    pub min_separate_denominator: u64,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
    pub pnl_numerator: u64,
    pub pnl_denominator: u64,
    pub swap_fee_numerator: u64,
    pub swap_fee_denominator: u64,
    pub base_need_take_pnl: u64,
    pub quote_need_take_pnl: u64,
    pub quote_total_pnl: u64,
    pub base_total_pnl: u64,
    pub quote_total_deposited: u128,
    pub base_total_deposited: u128,
    pub swap_base_in_amount: u128,
    pub swap_quote_out_amount: u128,
    pub swap_base2_quote_fee: u64,
    pub swap_quote_in_amount: u128,
    pub swap_base_out_amount: u128,
    pub swap_quote2_base_fee: u64,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub open_orders: Pubkey,
    pub market_id: Pubkey,
    pub market_program_id: Pubkey,
    pub target_orders: Pubkey,
    pub withdraw_queue: Pubkey,
    pub lp_vault: Pubkey,
    pub owner: Pubkey,
    pub pnl_owner: Pubkey,
}

impl LiquidityPool {
    /// Not yet implemented — the full 752-byte Raydium V4 layout parser is
    /// deferred. Returns a protocol error so any call site surfaces the
    /// missing work rather than silently succeeding.
    pub fn from_account_data(_data: &[u8]) -> Result<Self> {
        Err(Error::protocol_error(
            "RaydiumV4",
            "LiquidityPool::from_account_data is not yet implemented",
        ))
    }

    /// Spot price (quote per base) derived from the supplied reserves.
    #[must_use]
    pub fn calculate_spot_price(&self, base_reserve: u64, quote_reserve: u64) -> f64 {
        if base_reserve == 0 {
            return 0.0;
        }
        let base_ui = base_reserve as f64 / 10f64.powi(self.base_decimal as i32);
        let quote_ui = quote_reserve as f64 / 10f64.powi(self.quote_decimal as i32);
        quote_ui / base_ui
    }

    /// Status `3` is Raydium V4's canonical "initialized + active" state.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == 3
    }
}
