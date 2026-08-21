//! PumpSwap AMM protocol implementation.
//!
//! PumpSwap is the AMM that Pumpfun tokens graduate to after their bonding
//! curve completes. It uses a standard constant product formula (x * y = k)
//! with dynamic fees based on pool liquidity.
//!
//! # Architecture
//!
//! - Constant product AMM with inline reserves in pool state
//! - Dynamic fee structure: creator fees decrease as liquidity grows
//! - LP token support for liquidity providers
//! - Pool state is 301 bytes (8-byte discriminator + 293 bytes data)
//!
//! # Fee Structure
//!
//! Fees vary by SOL liquidity level (see `fee_structure()`):
//! - Protocol fee: 5 bps (constant)
//! - LP fee: varies (2-25 bps)
//! - Creator fee: varies (5-93 bps), decreases with liquidity
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::pumpswap::{PumpSwapPool, PumpSwapKeys};
//! use solana_protocols::traits::{SwapMath, SwapParams};
//!
//! let pool = PumpSwapPool::from_account_data(&account_data)?;
//! let output = pool.calculate_swap(&SwapParams::buy(1_000_000_000))?;
//! ```
//!
//! # Status: reference implementation
//!
//! This module is the shape every other protocol is migrating toward:
//! generated instruction dispatch, a derived and identity-checked account
//! layout, borsh events verified against the program IDL at compile time, and
//! extraction declared per instruction through the `Extracts*` traits with every
//! failure typed and counted.
//!
//! Copy from here when adding a protocol.

pub mod accounts;
mod constants;
pub mod events;
pub mod extract;
pub mod fee_config;
pub mod instructions;
mod math;
mod state;

#[cfg(feature = "cache-handlers")]
pub mod handler;

// Assembling a quote reads a cache, so this rides the same feature as the
// handlers that populate one.
#[cfg(feature = "cache-handlers")]
pub mod quote;

pub use accounts::{derive_creator_vault_authority, derive_user_volume_accumulator, PumpSwapKeys};
pub use constants::*;
pub use events::{BuyEvent, SellEvent, BUY_EVENT_DISCRIMINATOR, SELL_EVENT_DISCRIMINATOR};
pub use extract::PumpSwapExtractor;
pub use fee_config::PumpSwapFeeConfig;
pub use instructions::{
    BuyAccounts, BuyBuilder, BuyExactQuoteInAccounts, BuyExactQuoteInParams, BuyParams,
    CollectCoinCreatorFeeAccounts, CreatePoolAccounts, CreatePoolParams, DepositAccounts,
    DepositParams, PumpSwapInstruction, PumpSwapInstructionAccounts, PumpSwapInstructionEvent,
    SellAccounts, SellBuilder, SellParams, WithdrawAccounts, WithdrawParams,
};
pub use state::{PoolWithReserves, PumpSwapPool};
