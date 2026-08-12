//! Meteora DBC (Dynamic Bonding Curve) protocol.
//!
//! Program ID: `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN`
//!
//! Token launch platform with configurable bonding curves. Tokens graduate
//! to AMM pools (Meteora DAMM or DAMMv2) after hitting a quote reserve threshold.
//!
//! Key characteristics:
//! - **8-byte Anchor discriminators**: Standard Anchor instruction format
//! - **Two swap instructions**: swap (legacy exact-in) and swap2 (ExactIn/PartialFill/ExactOut)
//! - **Actual mints in accounts**: base_mint\[7\], quote_mint\[8\]
//! - **Token2022 support**: Separate base/quote token programs
//! - **Constant pool authority**: Same PDA for all pools
//! - **Referral system**: Optional referral token account at \[12\]
//!
//! # Current Implementation
//!
//! - Swap instruction parsing (swap + swap2 with all modes)
//! - State structs and curve math deferred

pub mod accounts;
pub mod constants;
mod instructions;
mod math;
pub mod state;

pub use accounts::DbcKeys;
pub use constants::*;
pub use instructions::{
    MeteoraDbcInstruction, Swap2Params, SwapAccounts, SwapBuilder, SwapBuilderConfig, SwapMode,
    SwapParams,
};
pub use math::{calculate_dbc_swap, DbcSwapPool, DEFAULT_FEE_RATE};
pub use state::VirtualPool;
