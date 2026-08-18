//! Meteora DLMM (Dynamic Liquidity Market Maker) — bin-based
//! concentrated liquidity AMM.
//!
//! Program id: `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo`.
//!
//! # Layout strategy
//!
//! Account, instruction, and type *layouts* come from the
//! codama-generated [`meteora-dlmm-sdk`] crate (re-exported here).
//! Everything around them — PDA derivation, math, event-log parsing,
//! instruction dispatch, cache handlers, the `ProtocolExtractor`
//! adapter — is hand-written in this module.
//!
//! Module structure mirrors `protocols/pumpfun/`:
//!
//! - [`constants`] — program id, event authority, account discriminators.
//! - [`accounts`] — PDA derivers + `DlmmKeys` bundle.
//! - [`state`] — re-exports SDK accounts; adds `LbPairExt` helper trait.
//! - [`extract`] — `ProtocolExtractor` impl (stub at step 1).
//! - [`handler`] — cache state handlers (`cache-handlers` feature only).
//! - `events/`, `instructions/`, `math/` — populated incrementally.
//!
//! [`meteora-dlmm-sdk`]: https://crates.io/crates/meteora-dlmm-sdk

//!
//! # Status: legacy shape
//!
//! Predates the current design: dispatch, account layouts and constants here
//! are hand-written rather than generated or derived. It decodes correctly and
//! is used in production, but a transcribed constant is this domain's most
//! expensive bug class and nothing here is checked against an IDL.
//!
//! **The reference implementation is `protocols::pumpfun`** — copy its shape,
//! not this one. See the crate README's coverage table for what differs.
pub mod accounts;
pub mod constants;
pub mod events;
pub mod extract;
pub mod instructions;
pub mod math;
pub mod positions;
pub mod state;

#[cfg(feature = "cache-handlers")]
pub mod handler;

// =====================================================================
// Public surface — kept narrow on purpose. Add to it as new modules
// (events, instructions, math) come online.
// =====================================================================

pub use accounts::{
    derive_bin_array_address, derive_bin_array_bitmap_extension, derive_oracle_address,
    derive_position_pda, event_authority, DlmmKeys,
};

pub use constants::{
    BASIS_POINT_MAX, BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR, BIN_ARRAY_BITMAP_SIZE,
    BIN_ARRAY_DISCRIMINATOR, CLAIM_FEE_OPERATOR_DISCRIMINATOR, EVENT_AUTHORITY, FEE_PRECISION,
    LB_PAIR_DISCRIMINATOR, MAX_BIN_ID, MAX_BIN_PER_ARRAY, MAX_FEE_RATE, MIN_BIN_ID, NUM_REWARDS,
    ORACLE_DISCRIMINATOR, POSITION_DISCRIMINATOR, POSITION_V2_DISCRIMINATOR,
    PRESET_PARAMETER2_DISCRIMINATOR, PRESET_PARAMETER_DISCRIMINATOR, PROGRAM_ID,
    TOKEN_BADGE_DISCRIMINATOR,
};

pub use extract::MeteoraDlmmExtractor;

pub use events::{find_event_body, parse_event_body, walk_events, MeteoraDlmmEvent};

pub use instructions::{parse as parse_instruction, MeteoraDlmmInstruction};

pub use math::{
    get_bin_array_pubkeys_for_swap, get_price_from_id, quote_exact_in, quote_exact_out,
    ExactInQuote, ExactOutQuote,
};

pub use positions::{
    bin_array_keys_for_position, bin_array_keys_for_range, compute_metrics, spot_balanced,
    spot_one_side, BalancedShape, OneSideShape, PositionMetrics, PositionRangeStatus,
    RangeStrategy, StrategySide,
};

pub use state::{
    decode_bin_array, decode_bin_array_bitmap_extension, decode_lb_pair, decode_position_v2,
    decode_position_v2_full, Bin, BinArray, BinArrayBitmapExtension, ClaimFeeOperator,
    DummyZcAccount, LbPair, LbPairExt, Oracle, PairStatus, PairType, Position, PositionBinData,
    PositionV2, PositionV2Full, PresetParameter, PresetParameter2, TokenBadge,
};
