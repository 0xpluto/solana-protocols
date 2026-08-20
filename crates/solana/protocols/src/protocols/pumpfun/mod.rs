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
pub use instructions::{
    create_ata_idempotent_instruction,
    create_ata_idempotent_instruction_for,
    create_ata_instruction,
    // Account types for parsing
    BuyAccounts,
    MigrateAccounts,
    MigrateParams,
    MigrateV2Accounts,
    MigrateV2Params,
    BuyV2Accounts,
    // Instruction builders
    BuyBuilder,
    BuyExactQuoteInV2Params,
    BuyExactSolInParams,
    BuyParams,
    BuyV2Params,
    CollectCreatorFeeParams,
    CollectCreatorFeeV2Params,
    CreateAccounts,
    CreateParams,
    CreateV2Accounts,
    CreateV2Params,
    DistributeCreatorFeesParams,
    DistributeCreatorFeesV2Params,
    // Unified instruction enum
    PumpfunInstruction,
    PumpfunInstructionAccounts,
    SellV2Accounts,
    PumpfunInstructionEvent,
    SellAccounts,
    SellBuilder,
    SellParams,
    SellV2Params,
};
pub use state::BondingCurve;
