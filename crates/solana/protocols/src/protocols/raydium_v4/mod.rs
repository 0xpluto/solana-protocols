//! Raydium V4 AMM protocol implementation.
//!
//! Raydium V4 is a legacy Solana AMM that predates Anchor. Key differences
//! from Anchor-based protocols:
//!
//! - **No discriminators in state**: Pool accounts start directly with data
//! - **1-byte instruction indices**: Instead of 8-byte Anchor discriminators
//! - **Serum integration**: Uses Serum DEX for order book matching
//!
//! # Architecture
//!
//! Raydium V4 uses constant product AMM formula (x * y = k) with a hybrid
//! approach that combines AMM liquidity with Serum order book.
//!
//! # Fee Structure
//!
//! Default trading fee is 0.25% (25 bps), configurable per pool.
//!
//! # Important Notes
//!
//! Unlike Pumpfun where reserves are in the pool state, Raydium V4 stores
//! reserves in separate token vault accounts. To calculate swaps, you need:
//! 1. The pool state (for fee config, decimals)
//! 2. The vault balances (fetched separately)
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::raydium_v4::{LiquidityPool, PoolWithReserves};
//! use solana_protocols::traits::{SwapMath, SwapParams};
//!
//! // Parse pool state (no discriminator!)
//! let pool = LiquidityPool::from_account_data(&pool_data)?;
//!
//! // Fetch vault balances separately
//! let base_reserve = fetch_token_balance(&pool.base_vault).await?;
//! let quote_reserve = fetch_token_balance(&pool.quote_vault).await?;
//!
//! // Calculate swap
//! let pool_with_reserves = PoolWithReserves::new(pool, base_reserve, quote_reserve);
//! let output = pool_with_reserves.calculate_swap(&SwapParams::buy(100_000_000))?;
//! ```

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
mod accounts;
mod constants;
mod instructions;
mod math;
mod state;

// Re-export types
pub use accounts::{derive_amm_id, derive_authority, RaydiumV4Keys};
pub use constants::*;
pub use instructions::{
    DepositAccounts, DepositParams, Initialize2Accounts, Initialize2Params, RaydiumV4Instruction,
    RaydiumV4InstructionAccounts, SwapBaseInAccounts, SwapBaseInBuilder, SwapBaseInParams,
    SwapBaseOutAccounts, SwapBaseOutParams, WithdrawAccounts, WithdrawParams,
};
pub use math::PoolWithReserves;
pub use state::LiquidityPool;
