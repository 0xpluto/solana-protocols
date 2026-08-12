//! Unified swap output and event types.
//!
//! All protocols produce [`SwapOutput`] from swap calculations,
//! providing consistent post-swap state tracking regardless of
//! the underlying protocol.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;

/// Unified swap output across all protocols.
///
/// This struct contains all information about a calculated or executed swap,
/// normalized to a common format regardless of the source protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwapOutput {
    /// Amount going into the swap (in smallest units).
    pub amount_in: u64,

    /// Amount coming out of the swap (in smallest units).
    pub amount_out: u64,

    /// Total fee paid (in input token units for buys, output for sells).
    pub fee: u64,

    /// Protocol fee portion.
    pub protocol_fee: u64,

    /// Creator/LP fee portion (if applicable).
    pub lp_fee: u64,

    /// Price impact in basis points (100 = 1%).
    pub price_impact_bps: u16,

    /// Effective price of this swap (output per input, adjusted for decimals).
    pub effective_price: f64,

    /// Spot price after the swap would execute.
    pub post_swap_price: f64,

    /// Accounts this quote consumed that the caller could not have known
    /// without running it — a CLMM's tick arrays, a DLMM's bin arrays.
    ///
    /// Empty for constant-product pools, and that is an accurate empty set
    /// rather than a placeholder: their quote reads nothing beyond the state
    /// it was assembled from.
    ///
    /// It lives on the *result* because only the quote knows which arrays the
    /// swap actually crossed — that depends on size. An instruction builder
    /// that re-derived them independently could disagree with the math the
    /// caller sized on, and would submit an instruction priced by a different
    /// walk than the one it quoted. Returning them makes the two one value.
    pub accounts_used: Vec<Pubkey>,
}

impl SwapOutput {
    /// Create a new swap output with all fields.
    #[must_use]
    pub fn new(amount_in: u64, amount_out: u64, fee: u64, protocol_fee: u64, lp_fee: u64) -> Self {
        SwapOutput {
            amount_in,
            amount_out,
            fee,
            protocol_fee,
            lp_fee,
            price_impact_bps: 0,
            effective_price: 0.0,
            post_swap_price: 0.0,
            // Constant-product quotes consume no dynamic accounts.
            accounts_used: Vec::new(),
        }
    }

    /// Create a simple swap output (for protocols with single fee).
    #[must_use]
    pub fn simple(amount_in: u64, amount_out: u64, fee: u64) -> Self {
        SwapOutput {
            amount_in,
            amount_out,
            fee,
            protocol_fee: fee,
            lp_fee: 0,
            price_impact_bps: 0,
            effective_price: 0.0,
            post_swap_price: 0.0,
            // Constant-product quotes consume no dynamic accounts.
            accounts_used: Vec::new(),
        }
    }

    /// Set the price impact.
    #[must_use]
    pub fn with_price_impact(mut self, impact_bps: u16) -> Self {
        self.price_impact_bps = impact_bps;
        self
    }

    /// Set effective price (for display).
    #[must_use]
    pub fn with_effective_price(mut self, price: f64) -> Self {
        self.effective_price = price;
        self
    }

    /// Set post-swap price.
    #[must_use]
    pub fn with_post_swap_price(mut self, price: f64) -> Self {
        self.post_swap_price = price;
        self
    }

    /// Calculate the minimum output with slippage tolerance.
    ///
    /// # Arguments
    /// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
    #[must_use]
    pub fn min_output(&self, slippage_bps: u16) -> u64 {
        let amount = self.amount_out as u128;
        let slippage = slippage_bps as u128;
        let result = (amount * (10000 - slippage)) / 10000;
        result as u64
    }

    /// Calculate the maximum input with slippage tolerance.
    ///
    /// # Arguments
    /// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
    #[must_use]
    pub fn max_input(&self, slippage_bps: u16) -> u64 {
        let amount = self.amount_in as u128;
        let slippage = slippage_bps as u128;
        let result = (amount * (10000 + slippage)) / 10000;
        result as u64
    }

    /// Check if this swap has significant price impact (> 1%).
    #[must_use]
    pub fn has_high_impact(&self) -> bool {
        self.price_impact_bps > 100
    }

    /// Get fee as percentage of input.
    #[must_use]
    pub fn fee_percentage(&self) -> f64 {
        if self.amount_in == 0 {
            0.0
        } else {
            (self.fee as f64 / self.amount_in as f64) * 100.0
        }
    }
}

impl Default for SwapOutput {
    fn default() -> Self {
        SwapOutput {
            amount_in: 0,
            amount_out: 0,
            fee: 0,
            protocol_fee: 0,
            lp_fee: 0,
            price_impact_bps: 0,
            effective_price: 0.0,
            post_swap_price: 0.0,
            // Constant-product quotes consume no dynamic accounts.
            accounts_used: Vec::new(),
        }
    }
}

/// Builder for constructing SwapOutput with fee breakdown.
#[derive(Debug, Default)]
pub struct SwapOutputBuilder {
    accounts_used: Vec<Pubkey>,
    amount_in: u64,
    amount_out: u64,
    protocol_fee: u64,
    lp_fee: u64,
    price_impact_bps: u16,
    effective_price: f64,
    post_swap_price: f64,
}

impl SwapOutputBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the input amount.
    #[must_use]
    pub fn amount_in(mut self, amount: u64) -> Self {
        self.amount_in = amount;
        self
    }

    /// Set the output amount.
    #[must_use]
    pub fn amount_out(mut self, amount: u64) -> Self {
        self.amount_out = amount;
        self
    }

    /// Set the protocol fee.
    #[must_use]
    pub fn protocol_fee(mut self, fee: u64) -> Self {
        self.protocol_fee = fee;
        self
    }

    /// Set the LP/creator fee.
    #[must_use]
    pub fn lp_fee(mut self, fee: u64) -> Self {
        self.lp_fee = fee;
        self
    }

    /// Set the price impact.
    #[must_use]
    pub fn price_impact_bps(mut self, bps: u16) -> Self {
        self.price_impact_bps = bps;
        self
    }

    /// Set the effective price.
    #[must_use]
    pub fn effective_price(mut self, price: f64) -> Self {
        self.effective_price = price;
        self
    }

    /// Set the post-swap price.
    #[must_use]
    pub fn post_swap_price(mut self, price: f64) -> Self {
        self.post_swap_price = price;
        self
    }

    /// Build the SwapOutput.
    #[must_use]
    pub fn build(self) -> SwapOutput {
        SwapOutput {
            amount_in: self.amount_in,
            amount_out: self.amount_out,
            fee: self.protocol_fee + self.lp_fee,
            protocol_fee: self.protocol_fee,
            lp_fee: self.lp_fee,
            price_impact_bps: self.price_impact_bps,
            effective_price: self.effective_price,
            post_swap_price: self.post_swap_price,
            accounts_used: self.accounts_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_output_slippage() {
        let output = SwapOutput::simple(1_000_000_000, 100_000_000, 1_000_000);

        // 1% slippage
        assert_eq!(output.min_output(100), 99_000_000);
        assert_eq!(output.max_input(100), 1_010_000_000);
    }

    #[test]
    fn swap_output_builder() {
        let output = SwapOutputBuilder::new()
            .amount_in(1_000_000_000)
            .amount_out(100_000_000)
            .protocol_fee(9_500_000)
            .lp_fee(3_000_000)
            .price_impact_bps(50)
            .build();

        assert_eq!(output.amount_in, 1_000_000_000);
        assert_eq!(output.amount_out, 100_000_000);
        assert_eq!(output.fee, 12_500_000); // Combined
        assert_eq!(output.protocol_fee, 9_500_000);
        assert_eq!(output.lp_fee, 3_000_000);
        assert_eq!(output.price_impact_bps, 50);
    }

    #[test]
    fn fee_percentage() {
        let output = SwapOutput::simple(1_000_000_000, 100_000_000, 12_500_000);
        let pct = output.fee_percentage();
        assert!((pct - 1.25).abs() < 0.001); // 1.25%
    }
}
