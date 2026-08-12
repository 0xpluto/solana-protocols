//! Core traits that all protocols must implement.
//!
//! These traits define the contract for protocol implementations:
//! - [`SwapMath`] - Calculate swap outputs from pool state
//! - [`InstructionBuilder`] - Build Solana instructions
//!
//! For state parsing, use the `#[derive(OnchainState)]` macro from
//! `solana_protocols_macros`. It generates:
//! - `DISCRIMINATOR: [u8; 8]` - Account discriminator
//! - `DATA_SIZE: usize` - Minimum account data size
//! - `from_account_data(&[u8]) -> Result<Self>` - Parse from raw bytes
//!
//! Each protocol implements these traits for their specific types,
//! enabling unified handling while preserving type safety.

use crate::events::SwapOutput;
use crate::Result;
use solana_program::pubkey::Pubkey;
use solana_sdk::instruction::Instruction;

/// Direction of a swap operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SwapDirection {
    /// Swap base token for quote token (buy quote with base).
    Buy,
    /// Swap quote token for base token (sell quote for base).
    Sell,
}

impl SwapDirection {
    /// Returns true if this is a buy operation.
    #[must_use]
    pub fn is_buy(&self) -> bool {
        matches!(self, SwapDirection::Buy)
    }

    /// Returns true if this is a sell operation.
    #[must_use]
    pub fn is_sell(&self) -> bool {
        matches!(self, SwapDirection::Sell)
    }

    /// Flip the direction.
    #[must_use]
    pub fn opposite(&self) -> Self {
        match self {
            SwapDirection::Buy => SwapDirection::Sell,
            SwapDirection::Sell => SwapDirection::Buy,
        }
    }
}

/// Amount specification for a swap.
///
/// Clearly distinguishes between specifying exact input vs exact output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapAmount {
    /// Exact input amount - calculate how much output you'll receive.
    ExactIn(u64),
    /// Exact output amount - calculate how much input is required.
    ExactOut(u64),
}

impl SwapAmount {
    /// Get the amount value.
    #[must_use]
    pub fn amount(&self) -> u64 {
        match self {
            SwapAmount::ExactIn(a) | SwapAmount::ExactOut(a) => *a,
        }
    }

    /// Check if this is an exact-in swap.
    #[must_use]
    pub fn is_exact_in(&self) -> bool {
        matches!(self, SwapAmount::ExactIn(_))
    }

    /// Check if this is an exact-out swap.
    #[must_use]
    pub fn is_exact_out(&self) -> bool {
        matches!(self, SwapAmount::ExactOut(_))
    }
}

/// Parameters for a swap calculation.
#[derive(Debug, Clone)]
pub struct SwapParams {
    /// The amount to swap (exact in or exact out).
    pub amount: SwapAmount,
    /// Direction of the swap.
    pub direction: SwapDirection,
}

impl SwapParams {
    /// Create params for a buy with exact input.
    #[must_use]
    pub fn buy(amount_in: u64) -> Self {
        SwapParams {
            amount: SwapAmount::ExactIn(amount_in),
            direction: SwapDirection::Buy,
        }
    }

    /// Create params for a sell with exact input.
    #[must_use]
    pub fn sell(amount_in: u64) -> Self {
        SwapParams {
            amount: SwapAmount::ExactIn(amount_in),
            direction: SwapDirection::Sell,
        }
    }

    /// Create params for a buy with exact output (how much input needed for X output).
    #[must_use]
    pub fn buy_exact_out(amount_out: u64) -> Self {
        SwapParams {
            amount: SwapAmount::ExactOut(amount_out),
            direction: SwapDirection::Buy,
        }
    }

    /// Create params for a sell with exact output (how much input needed for X output).
    #[must_use]
    pub fn sell_exact_out(amount_out: u64) -> Self {
        SwapParams {
            amount: SwapAmount::ExactOut(amount_out),
            direction: SwapDirection::Sell,
        }
    }

    /// Check if this is an exact-in swap.
    #[must_use]
    pub fn is_exact_in(&self) -> bool {
        self.amount.is_exact_in()
    }

    /// Check if this is an exact-out swap.
    #[must_use]
    pub fn is_exact_out(&self) -> bool {
        self.amount.is_exact_out()
    }

    /// Get the amount value.
    #[must_use]
    pub fn amount_value(&self) -> u64 {
        self.amount.amount()
    }
}

/// Trait for calculating swap outputs from pool state.
///
/// Each protocol implements this for their specific state type.
/// The implementation contains all protocol-specific math.
///
/// # Example
///
/// ```ignore
/// impl SwapMath for BondingCurve {
///     fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
///         // Protocol-specific constant product math
///     }
/// }
/// ```
pub trait SwapMath: Sized {
    /// Calculate swap output for given parameters.
    ///
    /// If `params.exact_out` is true, `params.amount_in` is treated as
    /// the desired output amount, and the result contains the required input.
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput>;

    /// Get the current spot price (quote per base).
    ///
    /// This is the marginal price at the current pool state,
    /// not accounting for swap size or fees.
    fn spot_price(&self) -> f64;

    /// Check if the pool is active and can execute swaps.
    fn is_active(&self) -> bool;
}

/// Trait for building Solana instructions.
///
/// Each protocol implements this to construct swap instructions
/// from pool keys and swap parameters.
///
/// # Type Parameters
///
/// - `Keys`: Protocol-specific pool keys/addresses needed for the instruction
/// - `Params`: Protocol-specific instruction parameters (amounts, slippage, etc.)
///
/// # Example
///
/// ```ignore
/// impl InstructionBuilder for PumpfunSwap {
///     type Keys = PumpfunKeys;
///     type Params = BuyParams;
///
///     fn build_swap_instruction(
///         keys: &Self::Keys,
///         user: &Pubkey,
///         params: Self::Params,
///     ) -> Result<Instruction> {
///         // Build the instruction
///     }
/// }
/// ```
pub trait InstructionBuilder {
    /// Protocol-specific pool keys/addresses.
    type Keys;

    /// Protocol-specific instruction parameters.
    type Params;

    /// Build the swap instruction.
    ///
    /// # Arguments
    ///
    /// * `keys` - Pool addresses and derived accounts
    /// * `user` - User's wallet address (signer)
    /// * `params` - Swap parameters (amounts, slippage, etc.)
    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> Result<Instruction>;

    /// Build swap instruction with optional ATA creation.
    ///
    /// Returns a vector of instructions:
    /// 1. (Optional) Create associated token account
    /// 2. The swap instruction
    ///
    /// Default implementation just returns the swap instruction.
    fn build_swap_with_setup(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
        _create_ata: bool,
    ) -> Result<Vec<Instruction>> {
        Ok(vec![Self::build_swap_instruction(keys, user, params)?])
    }
}

/// Slippage tolerance for swap operations.
///
/// Wraps a basis points value and provides methods to apply slippage
/// to amounts for calculating acceptable minimums/maximums.
///
/// # Example
///
/// ```ignore
/// let slippage = Slippage::new(100); // 1%
/// let min_out = slippage.apply_min(1_000_000); // 990_000
/// let max_in = slippage.apply_max(1_000_000);  // 1_010_000
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slippage {
    /// Slippage tolerance in basis points (100 = 1%).
    pub bps: u16,
}

impl Slippage {
    /// Create a new slippage tolerance.
    ///
    /// # Arguments
    /// * `bps` - Basis points (100 = 1%, 50 = 0.5%)
    #[must_use]
    pub const fn new(bps: u16) -> Self {
        Self { bps }
    }

    /// Create slippage from a percentage.
    ///
    /// # Arguments
    /// * `percent` - Percentage (1.0 = 1%, 0.5 = 0.5%)
    #[must_use]
    pub fn from_percent(percent: f64) -> Self {
        Self {
            bps: (percent * 100.0) as u16,
        }
    }

    /// Get slippage as a percentage.
    #[must_use]
    pub fn as_percent(&self) -> f64 {
        self.bps as f64 / 100.0
    }

    /// Apply slippage to get minimum acceptable output.
    ///
    /// Use this for the minimum amount you're willing to receive.
    #[must_use]
    pub fn apply_min(&self, amount: u64) -> u64 {
        let amount = amount as u128;
        let slippage = self.bps as u128;
        ((amount * (10000 - slippage)) / 10000) as u64
    }

    /// Apply slippage to get maximum acceptable input.
    ///
    /// Use this for the maximum amount you're willing to pay.
    #[must_use]
    pub fn apply_max(&self, amount: u64) -> u64 {
        let amount = amount as u128;
        let slippage = self.bps as u128;
        ((amount * (10000 + slippage)) / 10000) as u64
    }
}

impl Default for Slippage {
    /// Default slippage of 1% (100 bps).
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_direction() {
        assert!(SwapDirection::Buy.is_buy());
        assert!(!SwapDirection::Buy.is_sell());
        assert_eq!(SwapDirection::Buy.opposite(), SwapDirection::Sell);
    }

    #[test]
    fn swap_params_constructors() {
        let buy = SwapParams::buy(1_000_000);
        assert!(buy.direction.is_buy());
        assert!(buy.is_exact_in());
        assert_eq!(buy.amount_value(), 1_000_000);

        let sell = SwapParams::sell(500_000);
        assert!(sell.direction.is_sell());
        assert!(sell.is_exact_in());

        let buy_exact_out = SwapParams::buy_exact_out(100_000);
        assert!(buy_exact_out.direction.is_buy());
        assert!(buy_exact_out.is_exact_out());
        assert_eq!(buy_exact_out.amount_value(), 100_000);

        let sell_exact_out = SwapParams::sell_exact_out(50_000);
        assert!(sell_exact_out.direction.is_sell());
        assert!(sell_exact_out.is_exact_out());
    }

    #[test]
    fn swap_amount() {
        let exact_in = SwapAmount::ExactIn(1_000_000);
        assert!(exact_in.is_exact_in());
        assert!(!exact_in.is_exact_out());
        assert_eq!(exact_in.amount(), 1_000_000);

        let exact_out = SwapAmount::ExactOut(500_000);
        assert!(!exact_out.is_exact_in());
        assert!(exact_out.is_exact_out());
        assert_eq!(exact_out.amount(), 500_000);
    }

    #[test]
    fn slippage_calculations() {
        // 1% slippage on 1_000_000
        let slippage = Slippage::new(100);
        assert_eq!(slippage.apply_min(1_000_000), 990_000);
        assert_eq!(slippage.apply_max(1_000_000), 1_010_000);

        // 0.5% slippage
        let slippage = Slippage::new(50);
        assert_eq!(slippage.apply_min(1_000_000), 995_000);

        // From percent
        let slippage = Slippage::from_percent(1.0);
        assert_eq!(slippage.bps, 100);
        assert!((slippage.as_percent() - 1.0).abs() < 0.001);

        // Default is 1%
        assert_eq!(Slippage::default().bps, 100);
    }
}
