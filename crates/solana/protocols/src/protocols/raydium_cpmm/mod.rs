//! Raydium CPMM (Constant Product Market Maker) protocol.
//!
//! Program ID: `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C`
//!
//! Similar to Raydium V4 but with Token2022 support and simplified design.
//! Uses 8-byte Anchor discriminators.
//!
//! Key characteristics:
//! - **Constant product AMM**: x * y = k
//! - **Token2022 support**: Separate input/output token programs
//! - **Actual mints in accounts**: input_token_mint\[10\], output_token_mint\[11\]
//! - **Oracle integration**: Every swap updates observation state
//!
//! # Current Implementation
//!
//! - Swap instruction parsing (SwapBaseInput, SwapBaseOutput)
//! - Instruction building (SwapBaseInputBuilder)
//! - Pool state deserialization
//! - Swap math (constant product with configurable fees)
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::raydium_cpmm::{
//!     RaydiumCpmmKeys, SwapBaseInputBuilder, CpmmPoolWithReserves,
//! };
//!
//! let pool = CpmmPoolState::from_account_data(&data)?;
//! let keys = RaydiumCpmmKeys::from_pool_state(pool_address, &pool);
//! let ix = SwapBaseInputBuilder::buy(&keys, &user, 1_000_000_000, 900_000)?;
//! ```

pub mod accounts;
pub mod constants;
mod instructions;
mod math;
pub mod state;

pub use accounts::RaydiumCpmmKeys;
pub use constants::*;
pub use instructions::{
    RaydiumCpmmInstruction, SwapAccounts, SwapBaseInputBuilder, SwapBaseInputBuilderConfig,
    SwapBaseInputParams, SwapBaseOutputParams,
};
pub use math::{calculate_cpmm_swap, CpmmPoolWithReserves};
pub use state::CpmmPoolState;
