//! Raydium CLMM (Concentrated Liquidity Market Maker) protocol.
//!
//! Program ID: `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK`
//!
//! Raydium CLMM is a concentrated liquidity AMM similar to Uniswap V3.
//! Key differences from Raydium V4:
//!
//! - **Concentrated liquidity**: Providers set price ranges (ticks)
//! - **8-byte Anchor discriminators**: Standard Anchor instruction format
//! - **Tick arrays**: Price space divided into tick arrays for efficient lookups
//! - **Token2022 support** (via SwapV2): Handles both SPL Token and Token-2022
//!
//! # Architecture
//!
//! CLMM uses Q64.64 fixed-point sqrt_price representation. Ticks map to prices
//! via: `price = 1.0001^tick`. Each tick array holds 60 ticks.
//!
//! # Current Implementation
//!
//! - Swap + SwapV2 instruction parsing
//! - Pool state deserialization
//! - Instruction building (SwapV2Builder)
//! - Swap math (concentrated liquidity with tick crossing)
//! - Liquidity operations planned for future phase
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::raydium_clmm::{
//!     RaydiumClmmInstruction, RaydiumClmmKeys, SwapV2Builder, PoolState,
//! };
//!
//! let pool = PoolState::from_account_data(&data)?;
//! let keys = RaydiumClmmKeys::from_pool_state(pool_address, &pool);
//! let ix = SwapV2Builder::buy(&keys, &user, pool.tick_current, 1_000_000_000, 900_000);
//! ```

pub mod accounts;
mod constants;
mod instructions;
mod math;
mod state;

// Re-export types
pub use accounts::{derive_tick_array_address, tick_array_start_index, RaydiumClmmKeys};
pub use constants::*;
pub use instructions::{
    sqrt_price_x64_from_tick, RaydiumClmmInstruction, RaydiumClmmInstructionAccounts, SwapAccounts,
    SwapParams, SwapV2Accounts, SwapV2Builder, SwapV2BuilderConfig, SwapV2Params,
};
pub use math::{ClmmSwapComputer, PoolWithTickArrays};
pub use state::{PoolState, RewardInfo, TickArrayState, TickState};
