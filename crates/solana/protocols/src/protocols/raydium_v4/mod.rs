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
//! # Status: partial
//!
//! Instruction handling is the standard shape — one enum with
//! `#[derive(ProtocolInstruction)]`, one file per discriminator, params and
//! account structs on the derives — using `discriminator_size = 1`, since this
//! program indexes instructions by a single byte rather than an Anchor
//! discriminator. That width is a declared parameter, not a reason to diverge.
//!
//! Not yet standard: `LiquidityPool::from_account_data` is unimplemented (it
//! returns an error rather than guessing), there is no extractor, and nothing
//! here is verified against an IDL.
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
