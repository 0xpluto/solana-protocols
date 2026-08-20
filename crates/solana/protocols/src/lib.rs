//! Solana Protocol Implementations
//!
//! This crate provides compiler-driven protocol implementations for Solana DEX trading.
//! The design philosophy is **"compiler as checklist"** - adding a new protocol variant
//! generates compiler errors at every location requiring protocol-specific logic.
//!
//! # Quick Start
//!
//! ```
//! use solana_protocols::{PoolState, SwapMath, SwapParams};
//! use solana_protocols::parsing::state::Legacy;
//! use solana_protocols::pumpfun::BondingCurve;
//! use solana_program::pubkey::Pubkey;
//!
//! // Decoding is gated on the account's 8-byte discriminator, so arbitrary
//! // bytes are refused rather than misparsed into a plausible-looking pool.
//! assert!(PoolState::detect_and_parse(&[0u8; 128]).is_none());
//!
//! // Any decoded pool prices a swap through one trait, whatever the protocol.
//! // These are pump.fun's launch-state reserves.
//! let curve = BondingCurve {
//!     virtual_token_reserves: 1_073_000_000_000_000,
//!     virtual_quote_reserves: 30_000_000_000,
//!     real_token_reserves: 793_100_000_000_000,
//!     real_quote_reserves: 0,
//!     token_total_supply: 1_000_000_000_000_000,
//!     complete: false,
//!     creator: Pubkey::default(),
//!     // `Absent` means the account predates the cashback upgrade — which is
//!     // not the same fact as `Present(false)`.
//!     is_mayhem_mode: Legacy::Absent,
//!     is_cashback_coin: Legacy::Absent,
//! };
//!
//! // 1 SOL in, exact-in.
//! let out = curve.calculate_swap(&SwapParams::buy(1_000_000_000))?;
//! assert!(out.amount_out > 0);
//! assert!(out.fee > 0);
//! # Ok::<(), solana_protocols::Error>(())
//! ```
//!
//! Quoting from *live* state needs the accounts a pool depends on, which is
//! what [`solana-account-cache`] assembles. See that crate for the cached path.
//!
//! [`solana-account-cache`]: https://crates.io/crates/solana-account-cache
//!
//! # Core Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`Protocol`] | Enum of all supported protocols |
//! | [`PoolState`] | Unified pool state (wraps protocol-specific states) |
//! | [`PoolKeys`] | Unified pool keys (wraps protocol-specific keys) |
//! | [`SwapOutput`] | Unified swap calculation result |
//! | [`SwapParams`] | Swap parameters (direction, amount, exact-in/out) |
//!
//! # Traits
//!
//! | Trait | Description |
//! |-------|-------------|
//! | [`SwapMath`] | Calculate swap outputs from pool state |
//! | [`InstructionBuilder`] | Build Solana instructions |
//!
//! # Design Principles
//!
//! 1. **Exhaustive Matching**: No wildcard `_` patterns - every protocol must be
//!    explicitly handled everywhere. Adding a new protocol causes compiler errors.
//!
//! 2. **Trait-Based Abstraction**: Each protocol implements core traits:
//!    - [`SwapMath`] - Calculate swap outputs from pool state
//!    - [`InstructionBuilder`] - Build Solana instructions
//!    - `#[derive(OnchainState)]` - Parse on-chain account data (macro-generated)
//!
//! 3. **Unified Events**: All protocols produce [`SwapOutput`] for consistent
//!    post-swap state tracking.
//!
//! 4. **Minimal Boilerplate**: Derive macros reduce repetitive code:
//!    - `OnchainState` - Generates `from_account_data()` from the field list
//!    - `AccountMetas` - Generates `to_account_metas()` for instruction building
//!
//! # Adding a New Protocol
//!
//! See `PROTOCOL_IMPLEMENTATION_GUIDE.md` for detailed instructions.
//!
//! Quick checklist:
//! 1. Add variant to [`Protocol`] enum
//! 2. Add variant to [`PoolState`] enum
//! 3. Add variant to [`PoolKeys`] enum
//! 4. Fix all compiler errors (they guide you)
//! 5. Create protocol module implementing required traits
//!
//! # Modules
//!
//! - [`protocols`] - Protocol implementations (pumpfun, etc.)
//! - [`discovery`] - Protocol detection from account data
//! - [`tokens`] - Token program abstractions (SPL Token, Token 2022)
//! - [`traits`] - Core traits (`SwapMath`, `InstructionBuilder`)
//! - [`events`] - Unified event types (`SwapOutput`)
//! - [`error`] - Error types

// `#[derive(OnchainState)]` emits `::solana_protocols::…` paths so the derive
// works both inside this crate and out. This alias is what makes the former hold.
extern crate self as solana_protocols;

// Quote bundles + the cross-protocol dispatch over them. Reads a cache, so it
// rides the same feature as the handlers that populate one.
#[cfg(feature = "cache-handlers")]
pub mod quote;

// Grading quote math against executed swaps. Pure — no I/O, no tape.
pub mod verify;

pub mod chain;
pub mod discovery;
pub mod error;
pub mod events;
pub mod idl;
pub mod metaplex;
pub mod pairs;
pub mod parsing;
pub mod platform;
pub mod protocols;
pub mod swap_instruction;
/// Token-account cache handler (vault balances — the AMM reserve feed).
#[cfg(feature = "cache-handlers")]
pub mod token_handler;
pub mod tokens;
pub mod traits;
pub mod undecoded;

/// Golden on-chain fixture harness for decoder verification (test-only).
#[cfg(test)]
pub(crate) mod test_fixtures;

/// Completeness guard: every Anchor account handler must carry a
/// [`VerifiedDecoder`](solana_account_traits::VerifiedDecoder) proof — a compile-time
/// derived discriminator plus a golden fixture, both from
/// `#[derive(OnchainAccount)]`. This is a compile-time check in test form:
/// adding an Anchor handler (or converting one) without the derive fails to
/// compile here. `register` itself stays open to non-Anchor handlers by design.
#[cfg(all(test, feature = "cache-handlers"))]
mod verified_decoder_completeness {
    fn assert_verified<H: solana_account_traits::VerifiedDecoder>() {}

    /// The declared set of Anchor account handlers.
    ///
    /// Hand-maintained, and [`the_declared_set_is_every_handler_in_the_tree`]
    /// is what keeps it honest.
    ///
    /// [`the_declared_set_is_every_handler_in_the_tree`]:
    ///     Self::the_declared_set_is_every_handler_in_the_tree
    const DECLARED: [&str; 8] = [
        "PumpSwapPoolHandler",
        "PumpfunBondingCurveHandler",
        "PumpfunGlobalHandler",
        "PumpfunFeeConfigHandler",
        "LbPairHandler",
        "BinArrayHandler",
        "BinArrayBitmapExtensionHandler",
        "PositionV2Handler",
    ];

    /// Every declared handler carries the derive's proof: a compile-time
    /// discriminator plus a golden fixture. A handler listed here without
    /// `#[derive(OnchainAccount)]` fails to compile.
    #[test]
    fn every_declared_handler_is_verified() {
        use crate::protocols::meteora_dlmm::handler::{
            BinArrayBitmapExtensionHandler, BinArrayHandler, LbPairHandler, PositionV2Handler,
        };
        use crate::protocols::pumpfun::handler::{
            PumpfunBondingCurveHandler, PumpfunFeeConfigHandler, PumpfunGlobalHandler,
        };
        use crate::protocols::pumpswap::handler::PumpSwapPoolHandler;

        assert_verified::<PumpSwapPoolHandler>();
        assert_verified::<PumpfunBondingCurveHandler>();
        assert_verified::<PumpfunGlobalHandler>();
        assert_verified::<PumpfunFeeConfigHandler>();
        assert_verified::<LbPairHandler>();
        assert_verified::<BinArrayHandler>();
        assert_verified::<BinArrayBitmapExtensionHandler>();
        assert_verified::<PositionV2Handler>();
    }

    /// The declared set is *every* handler in the tree, not merely a set that
    /// happens to compile.
    ///
    /// Without this the list above proves nothing about a handler nobody added
    /// to it: a new `#[derive(OnchainAccount)]` type would be unverified,
    /// unregistered, and completely silent — the check-that-does-not-run shape,
    /// in the file whose whole job is preventing it.
    ///
    /// Reads the source because there is no link-time inventory to ask. The
    /// alternative — a `handlers!` macro owning the list — moves the same
    /// hand-maintenance somewhere tidier without closing the hole.
    #[test]
    fn the_declared_set_is_every_handler_in_the_tree() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/protocols");
        let mut found = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("protocols dir is readable") {
                let path = entry.expect("readable entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("source is readable");
                // `#[derive(… OnchainAccount)]` … `pub struct <Name>`
                for (idx, _) in src.match_indices("OnchainAccount)]") {
                    let after = &src[idx..];
                    let name = after
                        .split("struct ")
                        .nth(1)
                        .expect("a derive is followed by its struct")
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .expect("struct name")
                        .to_string();
                    found.push(name);
                }
            }
        }
        found.sort();
        found.dedup();
        let mut declared: Vec<String> = DECLARED.iter().map(|s| (*s).to_string()).collect();
        declared.sort();
        assert_eq!(
            found, declared,
            "a handler in the tree is missing from DECLARED (or vice versa) — \
             add it here, to the registration in the consuming binary, and to \
             every_declared_handler_is_verified"
        );
    }
}

// Re-export core types at crate root
pub use discovery::{
    detect_protocol_from_data, detect_protocol_from_program, is_protocol_account,
    protocol_account_size, protocol_discriminator, Discriminator,
};
pub use error::{Error, Result};
pub use events::SwapOutput;
pub use protocols::{PoolKeys, PoolState, Protocol};
pub use tokens::{
    BaseToken,
    TokenAccount,
    TokenAccountBuilder,
    TokenProgram,
    TokenWithProgram,
    // Well-known token mints
    BONK,
    JUP,
    RAY,
    USDC,
    USDT,
    WSOL,
};
pub use traits::{InstructionBuilder, Slippage, SwapAmount, SwapDirection, SwapMath, SwapParams};

// Re-export semantic chain events — the consumer-facing output of the
// transaction parser. See `chain/` for the end-data design. Per-protocol
// extractor adapters (e.g. `PumpfunExtractor`) live with their protocol
// module and are re-exported from there (`pumpfun::PumpfunExtractor`).
pub use chain::{
    extract_transaction, ChainEvent, CurveState, ExtractContext, ExtractFn, ExtractorRegistry,
    Migration, NoContext, ParsedTransaction, ProtocolExtractor, Swap, TokenCreation,
    TransactionHeader, TxError, TxOutcome,
};

// Re-export derive macros
// OnchainState generates from_account_data() for on-chain state structs.
pub use solana_protocols_macros::{AccountMetas, InstructionData, LogParser, OnchainState};

// Re-export parsing types (Layer 1: Generic parsing)
pub use parsing::{
    FromAccountKeys, FromInstructionData, FromLogData, InstructionParseError, LogEntry,
    LogParseError, LogParser, ParsedInstruction, ParsedInstructionBuilder, ParsedLog,
    ProtocolEvent, ProtocolParser, ProtocolRegistry, SourceError, SwapEvent, TokenBalanceChange,
    TokenCreationEvent, TransactionMeta, TransactionSource,
};

// Re-export classification traits (Layer 2: Semantic classification)
pub use parsing::{
    ClassifiesAsSwap, ClassifiesAsTokenCreation, SwapAmount as ClassifiedSwapAmount,
    SwapClassification, TokenCreationClassification,
};

// Re-export protocol modules for convenience
pub use protocols::meteora_dbc;
pub use protocols::meteora_dlmm;
pub use protocols::pumpfun;
pub use protocols::pumpswap;
pub use protocols::raydium_clmm;
pub use protocols::raydium_cpmm;
pub use protocols::raydium_launchpad;
pub use protocols::raydium_v4;
pub use protocols::spl_token;
