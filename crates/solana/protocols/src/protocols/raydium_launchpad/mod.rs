//! Raydium Launchpad protocol (bonding curve with graduation).
//!
//! Program ID: `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj`
//!
//! Token launch platform with configurable bonding curves (constant product,
//! fixed price, linear price). Supports buy/sell in both exact-in and exact-out
//! modes with fee splitting between protocol, platform, and creator.
//!
//! Key characteristics:
//! - **8-byte Anchor discriminators**: Standard Anchor instruction format
//! - **4 swap variants**: BuyExactIn, BuyExactOut, SellExactIn, SellExactOut
//! - **Actual mints in accounts**: base_token_mint\[9\], quote_token_mint\[10\]
//! - **Token2022 support**: Separate base/quote token programs
//! - **Share fee rate**: Optional extra fee in swap params (usually 0)
//!
//! # Current Implementation
//!
//! - Swap instruction parsing (all 4 variants)
//! - Instruction building (BuyExactInBuilder, SellExactInBuilder)
//! - Pool state deserialization
//! - Swap math (bonding curve with virtual reserves)
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::raydium_launchpad::{
//!     LaunchpadKeys, BuyExactInBuilder, LaunchpadSwapPool,
//! };
//!
//! let pool = LaunchpadPoolState::from_account_data(&data)?;
//! let keys = LaunchpadKeys::from_pool_state(pool_address, &pool);
//! let ix = BuyExactInBuilder::buy(&keys, &user, 1_000_000_000, 900_000)?;
//! ```

//!
//! # Status: partial
//!
//! Instruction dispatch is generated and the account layout is derived and
//! identity-checked, but events and extraction are still hand-written and no
//! IDL verification runs against this program. Correct as far as it goes.
//!
//! **The reference implementation is `protocols::pumpfun`** — copy its shape,
//! not this one. See the crate README's coverage table for what differs.
pub mod accounts;
pub mod constants;
mod instructions;
mod math;
pub mod state;

pub use accounts::{derive_authority, resolve_token_programs, LaunchpadKeys};
pub use constants::*;
pub use instructions::{
    BuyExactInAccounts, BuyExactInBuilder, BuyExactInParams, BuyExactOutAccounts,
    BuyExactOutParams, RaydiumLaunchpadInstruction, SellExactInAccounts, SellExactInBuilder,
    SellExactInParams, SellExactOutAccounts, SellExactOutParams, SwapAccounts,
};
pub use math::{calculate_launchpad_swap, LaunchpadSwapPool};
pub use state::LaunchpadPoolState;
