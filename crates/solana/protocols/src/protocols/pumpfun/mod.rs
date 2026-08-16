//! Pump.fun bonding curve protocol implementation.
//!
//! This module provides direct interaction with pump.fun bonding curves,
//! bypassing the PumpPortal API entirely.
//!
//! # Architecture
//!
//! Pump.fun uses a constant product AMM formula for its bonding curve:
//! - `x * y = k` where x is virtual SOL reserves and y is virtual token reserves
//! - Fees are deducted from the input amount (for buys) or output amount (for sells)
//!
//! # Fee Structure
//!
//! - Protocol fee: 0.95% (95 bps)
//! - Creator fee: 0.30% (30 bps)
//! - Total: 1.25% (125 bps)
//!
//! # Example
//!
//! ```ignore
//! use solana_protocols::pumpfun::{BondingCurve, BuyBuilder};
//! use solana_protocols::traits::{SwapMath, SwapParams};
//!
//! // Parse bonding curve from on-chain account data
//! let curve = BondingCurve::from_account_data(&account_data)?;
//!
//! // Calculate how many tokens you get for 1 SOL
//! let params = SwapParams::buy(1_000_000_000); // 1 SOL in lamports
//! let output = curve.calculate_swap(&params)?;
//!
//! // Build the buy instruction
//! let keys = PumpfunKeys::new(mint, curve.creator);
//! let ix = BuyBuilder::build_swap_instruction(
//!     &keys,
//!     &user_wallet,
//!     BuyParams::new(output.amount_out, output.amount_in),
//! )?;
//! ```

mod accounts;
mod constants;
pub mod events;
mod fee_config;
mod global;
mod instructions;
mod math;
mod state;

#[cfg(feature = "cache-handlers")]
pub mod handler;

// Assembling a quote reads a cache, so this rides the same feature.
#[cfg(feature = "cache-handlers")]
pub mod quote;

pub mod extract;

pub use extract::PumpfunExtractor;

// Re-export everything
pub use accounts::{
    derive_associated_bonding_curve, derive_bonding_curve_pda, derive_creator_vault_pda,
    derive_user_volume_accumulator_pda, PumpfunKeys,
};
pub use constants::*;
pub use events::TRADE_EVENT_DISCRIMINATOR;
pub use fee_config::{
    bonding_curve_market_cap, calculate_fee_tier, PumpfunFeeConfig, PumpfunFeeTier, PumpfunFees,
};
pub use global::{PumpfunFeeRecipients, PumpfunGlobal};
pub mod track_volume;
pub use track_volume::{TrackVolume, UnknownTrackVolume};

pub use instructions::{
    create_ata_idempotent_instruction,
    create_ata_idempotent_instruction_for,
    create_ata_instruction,
    // Account types for parsing
    BuyAccounts,
    // Instruction builders
    BuyBuilder,
    BuyExactInParams,
    BuyParams,
    CreateAccounts,
    CreateParams,
    CreateV2Accounts,
    CreateV2Params,
    // Unified instruction enum
    PumpfunInstruction,
    PumpfunInstructionAccounts,
    PumpfunInstructionEvent,
    SellAccounts,
    SellBuilder,
    SellParams,
};
pub use state::BondingCurve;
