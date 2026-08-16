//! Transaction parsing utilities.
//!
//! This module provides improved abstractions for parsing Solana transactions
//! from multiple sources (RPC, GRPC). Key improvements over legacy code:
//!
//! - **No Rc/RefCell**: Flat vector with parent indices instead of tree
//! - **Result-based errors**: No panics in parsing code
//! - **Cached regex**: Lazy-static patterns for log parsing
//! - **O(1) protocol dispatch**: Registry pattern with HashMap lookup
//! - **Discriminator-first log parsing**: Match on first 8 bytes
//!
//! # Architecture (Two Layers)
//!
//! ## Layer 1: Generic Parsing
//!
//! Raw instruction/accounts/log data extraction:
//! - [`ParsedInstruction`] - Instruction with resolved accounts and associated logs
//! - [`LogEntry`] - Structured log entry (Data, Invoke, Success, etc.)
//! - [`ProtocolRegistry`] - O(1) dispatch to protocol-specific parsers
//!
//! ## Layer 2: Classification
//!
//! Semantic classification of parsed events:
//! - [`ClassifiesAsSwap`] - "Is this a swap?" → [`SwapClassification`]
//! - [`ClassifiesAsTokenCreation`] - "Is this token creation?" → [`TokenCreationClassification`]
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::parsing::{ProtocolRegistry, ClassifiesAsSwap};
//! use solana_protocols::pumpfun::PumpfunInstruction;
//!
//! // Parse instruction
//! let instruction = PumpfunInstruction::try_from_slice(&ix_data)?;
//!
//! // Layer 2: Classify
//! if let Some(swap) = instruction.as_swap() {
//!     println!("Swap direction: {:?}", swap.direction);
//!     println!("Input amount: {:?}", swap.amount_in);
//! }
//! ```

pub mod anchor;
mod classify;
pub mod event;
mod instruction;
mod log;
pub mod log_fixture;
mod registry;
mod source;
pub mod state;
mod traits;

// Layer 1: Generic parsing
pub use instruction::{
    attribution_stats, parse_instructions, AttributionStats, ParsedInstruction,
    ParsedInstructionBuilder, RawInstruction,
};
pub use log::{parse_logs, LogEntry, LogParseError, LogParser, ParsedLog};
pub use log_fixture::{
    audit_log_slice, capture_fixture, capture_imperfect, deep_audit, deep_audit_tally,
    FixtureError, FixtureInstruction, LogFixture, LogSliceVerdict, VERDICT_LABELS,
};
pub use registry::{
    InstructionParseError, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreationEvent,
    ProtocolEvent, ProtocolParser, ProtocolRegistry, SwapEvent, TokenCreationEvent,
};
pub use source::{SourceError, TokenBalanceChange, TransactionMeta, TransactionSource};
pub use traits::{
    FromAccountKeys, FromInstructionData, FromLogData, NoParams, VerifiedInstruction,
};

// Layer 2: Classification
pub use classify::{
    ClassifiesAsSwap, ClassifiesAsTokenCreation, SwapAmount, SwapClassification,
    TokenCreationClassification,
};
