//! Classification traits for parsed protocol events.
//!
//! This module provides **Layer 2** classification on top of generic parsing.
//! While Layer 1 (generic parsing) gives you the raw instruction/accounts/log data,
//! Layer 2 (classification) answers the question: "What kind of action is this?"
//!
//! # Architecture
//!
//! ```text
//! Layer 1: Generic Parsing
//! ┌─────────────────────────────────────────────────────────────┐
//! │  ParsedInstruction → PumpfunInstruction + Accounts + Log    │
//! │  (instruction data, account pubkeys, log bytes)             │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! Layer 2: Classification
//! ┌─────────────────────────────────────────────────────────────┐
//! │  "Is this a swap?" → ClassifiesAsSwap::as_swap()            │
//! │  "Is this token creation?" → ClassifiesAsTokenCreation      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::parsing::{ClassifiesAsSwap, SwapClassification};
//! use solana_protocols::pumpfun::PumpfunInstruction;
//!
//! let instruction = PumpfunInstruction::try_from_slice(&data)?;
//!
//! if let Some(swap) = instruction.as_swap() {
//!     println!("Swap direction: {:?}", swap.direction);
//!     println!("Input amount: {}", swap.amount_in);
//! }
//! ```

use crate::traits::SwapDirection;
use solana_program::pubkey::Pubkey;

// =============================================================================
// Swap Classification
// =============================================================================

/// Classification data for a swap instruction.
///
/// This struct provides a unified view of swap parameters across all protocols.
/// It's returned by [`ClassifiesAsSwap::as_swap()`].
#[derive(Debug, Clone)]
pub struct SwapClassification {
    /// Direction of the swap (buy or sell).
    pub direction: SwapDirection,
    /// Input amount (what the user sends).
    /// For buys: SOL amount. For sells: token amount.
    pub amount_in: SwapAmount,
    /// Output amount or minimum (what the user receives).
    /// For buys: min tokens. For sells: min SOL.
    pub amount_out: SwapAmount,
    /// Token mint being traded.
    pub mint: Option<Pubkey>,
    /// Pool/curve address.
    pub pool: Option<Pubkey>,
    /// User performing the swap.
    pub user: Option<Pubkey>,
}

/// Amount specification in a swap classification.
#[derive(Debug, Clone, Copy)]
pub enum SwapAmount {
    /// Exact amount specified.
    Exact(u64),
    /// Minimum amount (slippage protection).
    Minimum(u64),
    /// Maximum amount (slippage protection).
    Maximum(u64),
    /// Unknown amount (must be derived from log).
    Unknown,
}

impl SwapClassification {
    /// Create a new swap classification.
    #[must_use]
    pub fn new(direction: SwapDirection, amount_in: SwapAmount, amount_out: SwapAmount) -> Self {
        Self {
            direction,
            amount_in,
            amount_out,
            mint: None,
            pool: None,
            user: None,
        }
    }

    /// Set the mint address.
    #[must_use]
    pub fn with_mint(mut self, mint: Pubkey) -> Self {
        self.mint = Some(mint);
        self
    }

    /// Set the pool address.
    #[must_use]
    pub fn with_pool(mut self, pool: Pubkey) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Set the user address.
    #[must_use]
    pub fn with_user(mut self, user: Pubkey) -> Self {
        self.user = Some(user);
        self
    }
}

/// Trait for types that can be classified as a swap.
///
/// Implement this for instruction enums or event types to enable
/// unified swap handling across protocols.
pub trait ClassifiesAsSwap {
    /// Try to classify this as a swap.
    ///
    /// Returns `Some(SwapClassification)` if this represents a swap,
    /// `None` otherwise (e.g., for CREATE instructions).
    fn as_swap(&self) -> Option<SwapClassification>;

    /// Check if this is a swap.
    fn is_swap(&self) -> bool {
        self.as_swap().is_some()
    }
}

// =============================================================================
// Token Creation Classification
// =============================================================================

/// Classification data for a token creation instruction.
#[derive(Debug, Clone)]
pub struct TokenCreationClassification {
    /// Token mint address.
    pub mint: Pubkey,
    /// Token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
    /// Metadata URI.
    pub uri: String,
    /// Creator address.
    pub creator: Option<Pubkey>,
    /// Pool/curve address (if applicable).
    pub pool: Option<Pubkey>,
}

impl TokenCreationClassification {
    /// Create a new token creation classification.
    #[must_use]
    pub fn new(mint: Pubkey, name: String, symbol: String, uri: String) -> Self {
        Self {
            mint,
            name,
            symbol,
            uri,
            creator: None,
            pool: None,
        }
    }

    /// Set the creator address.
    #[must_use]
    pub fn with_creator(mut self, creator: Pubkey) -> Self {
        self.creator = Some(creator);
        self
    }

    /// Set the pool address.
    #[must_use]
    pub fn with_pool(mut self, pool: Pubkey) -> Self {
        self.pool = Some(pool);
        self
    }
}

/// Trait for types that can be classified as token creation.
pub trait ClassifiesAsTokenCreation {
    /// Try to classify this as token creation.
    ///
    /// Returns `Some(TokenCreationClassification)` if this represents
    /// a token creation, `None` otherwise.
    fn as_token_creation(&self) -> Option<TokenCreationClassification>;

    /// Check if this is a token creation.
    fn is_token_creation(&self) -> bool {
        self.as_token_creation().is_some()
    }
}

// =============================================================================
// Pumpfun Implementations
// =============================================================================

use crate::protocols::pumpfun::{PumpfunInstruction, PumpfunInstructionEvent};

impl ClassifiesAsSwap for PumpfunInstruction {
    fn as_swap(&self) -> Option<SwapClassification> {
        match self {
            // `buy`/`buy_v2` pin the TOKENS OUT and ceiling the SOL. The out
            // side read `Minimum` here, which had the pinned side backwards.
            PumpfunInstruction::Buy(params) | PumpfunInstruction::BuyV2(params) => {
                Some(SwapClassification::new(
                    SwapDirection::Buy,
                    SwapAmount::Maximum(params.max_sol_cost),
                    SwapAmount::Exact(params.amount),
                ))
            }
            // The exact-IN buys are the mirror image: SOL in is pinned, tokens
            // out are a floor. Same bytes, opposite meaning — which is why
            // they carry their own params type.
            PumpfunInstruction::BuyExactSolIn(params)
            | PumpfunInstruction::BuyExactQuoteInV2(params) => Some(SwapClassification::new(
                SwapDirection::Buy,
                SwapAmount::Exact(params.spendable_in),
                SwapAmount::Minimum(params.min_tokens_out),
            )),
            PumpfunInstruction::Sell(params) | PumpfunInstruction::SellV2(params) => {
                Some(SwapClassification::new(
                    SwapDirection::Sell,
                    SwapAmount::Exact(params.amount),
                    SwapAmount::Minimum(params.min_sol_output),
                ))
            }
            PumpfunInstruction::Create(_) | PumpfunInstruction::CreateV2(_) => None,
        }
    }
}

impl ClassifiesAsTokenCreation for PumpfunInstruction {
    fn as_token_creation(&self) -> Option<TokenCreationClassification> {
        match self {
            PumpfunInstruction::Create(params) => {
                // Note: mint is not in params, only in accounts
                Some(TokenCreationClassification::new(
                    Pubkey::default(), // Will be filled from accounts
                    params.name.clone(),
                    params.symbol.clone(),
                    params.uri.clone(),
                ))
            }
            PumpfunInstruction::CreateV2(params) => Some(TokenCreationClassification::new(
                Pubkey::default(),
                params.name.clone(),
                params.symbol.clone(),
                params.uri.clone(),
            )),
            PumpfunInstruction::Buy(_)
            | PumpfunInstruction::BuyV2(_)
            | PumpfunInstruction::BuyExactSolIn(_)
            | PumpfunInstruction::BuyExactQuoteInV2(_)
            | PumpfunInstruction::Sell(_)
            | PumpfunInstruction::SellV2(_) => None,
        }
    }
}

impl ClassifiesAsSwap for PumpfunInstructionEvent {
    fn as_swap(&self) -> Option<SwapClassification> {
        self.instruction.as_swap().map(|mut swap| {
            swap.mint = Some(self.accounts.mint());
            swap.pool = Some(self.accounts.bonding_curve());
            swap.user = Some(self.accounts.user());
            swap
        })
    }
}

impl ClassifiesAsTokenCreation for PumpfunInstructionEvent {
    fn as_token_creation(&self) -> Option<TokenCreationClassification> {
        self.instruction.as_token_creation().map(|mut creation| {
            creation.mint = self.accounts.mint();
            creation.pool = Some(self.accounts.bonding_curve());
            creation.creator = Some(self.accounts.user());
            creation
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::pumpfun::{BuyParams, CreateParams, SellParams};

    #[test]
    fn pumpfun_buy_classifies_as_swap() {
        let instruction = PumpfunInstruction::Buy(BuyParams::new(1_000_000, 100_000));

        let swap = instruction.as_swap().expect("should classify as swap");
        assert!(matches!(swap.direction, SwapDirection::Buy));
        assert!(matches!(swap.amount_in, SwapAmount::Maximum(100_000)));
        // `buy` PINS the tokens out — the program delivers exactly this or
        // fails. This asserted `Minimum` until 2026-08-11, which named the
        // wrong side as pinned and is the distinction the exact-in variants
        // exist to make.
        assert!(matches!(swap.amount_out, SwapAmount::Exact(1_000_000)));
    }

    /// The exact-IN buy pins the opposite side from `buy`.
    #[test]
    fn pumpfun_exact_sol_in_pins_the_input() {
        use crate::protocols::pumpfun::BuyExactInParams;
        let ix = PumpfunInstruction::BuyExactSolIn(BuyExactInParams {
            spendable_in: 500_000,
            min_tokens_out: 9,
            track_volume: crate::protocols::OptionBool::None,
        });
        let swap = ix.as_swap().expect("classifies");
        assert!(matches!(swap.amount_in, SwapAmount::Exact(500_000)));
        assert!(matches!(swap.amount_out, SwapAmount::Minimum(9)));
    }

    #[test]
    fn pumpfun_sell_classifies_as_swap() {
        let instruction = PumpfunInstruction::Sell(SellParams::new(1_000_000, 50_000));

        let swap = instruction.as_swap().expect("should classify as swap");
        assert!(matches!(swap.direction, SwapDirection::Sell));
        assert!(matches!(swap.amount_in, SwapAmount::Exact(1_000_000)));
        assert!(matches!(swap.amount_out, SwapAmount::Minimum(50_000)));
    }

    #[test]
    fn pumpfun_create_does_not_classify_as_swap() {
        let instruction = PumpfunInstruction::Create(CreateParams::new(
            "Test".to_string(),
            "TST".to_string(),
            "https://example.com".to_string(),
        ));

        assert!(instruction.as_swap().is_none());
        assert!(!instruction.is_swap());
    }

    #[test]
    fn pumpfun_create_classifies_as_token_creation() {
        let instruction = PumpfunInstruction::Create(CreateParams::new(
            "Test Token".to_string(),
            "TEST".to_string(),
            "https://example.com/meta.json".to_string(),
        ));

        let creation = instruction
            .as_token_creation()
            .expect("should classify as token creation");
        assert_eq!(creation.name, "Test Token");
        assert_eq!(creation.symbol, "TEST");
        assert_eq!(creation.uri, "https://example.com/meta.json");
    }

    #[test]
    fn pumpfun_buy_does_not_classify_as_token_creation() {
        let instruction = PumpfunInstruction::Buy(BuyParams::new(1_000_000, 100_000));

        assert!(instruction.as_token_creation().is_none());
        assert!(!instruction.is_token_creation());
    }

    #[test]
    fn swap_classification_builder() {
        let swap = SwapClassification::new(
            SwapDirection::Buy,
            SwapAmount::Exact(1_000_000_000),
            SwapAmount::Minimum(100_000),
        )
        .with_mint(Pubkey::new_unique())
        .with_pool(Pubkey::new_unique())
        .with_user(Pubkey::new_unique());

        assert!(swap.mint.is_some());
        assert!(swap.pool.is_some());
        assert!(swap.user.is_some());
    }
}
