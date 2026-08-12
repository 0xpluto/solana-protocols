//! Bin-walk swap math for Meteora DLMM.
//!
//! Reimplemented from the on-chain Meteora DLMM (`lb_clmm`) program,
//! over the SDK's borsh-derived account/type structs as the data layer.
//! Splits into:
//!
//! * Pure fixed-point primitives: [`safe_math`], [`u128x128`],
//!   [`u64x64`], [`util`].
//! * Pricing: [`price`] — bin id → Q64.64 price.
//! * Per-account math (free functions over the SDK structs):
//!   [`lb_pair`] (fee + volatility update), [`bin`] (per-bin swap
//!   step), [`bin_array`] (bin-id ↔ array index), [`bitmap`]
//!   (bin-array discovery via the inline + extension bitmaps).
//! * Top-level quoter: [`quote`] — `quote_exact_in` / `quote_exact_out`
//!   plus `get_bin_array_pubkeys_for_swap` for routing.
//!
//! The quoter takes an owned snapshot — caller passes `&LbPair` and
//! `HashMap<Pubkey, BinArray>` (typically lifted out of the live
//! cache). Internally we clone before walking, so the caller's data
//! is never mutated.
//!
//! ## Example
//!
//! ```rust,ignore
//! use std::collections::HashMap;
//! use solana_program::pubkey::Pubkey;
//! use solana_protocols::meteora_dlmm::math::{quote_exact_in, ExactInQuote};
//! use solana_protocols::meteora_dlmm::{LbPair, BinArray};
//!
//! let pair: LbPair = /* … */;
//! let pool: Pubkey = /* … */;
//! let bin_arrays: HashMap<Pubkey, BinArray> = /* loaded from cache */;
//!
//! let ExactInQuote { amount_out, fee } = quote_exact_in(
//!     pool,
//!     &pair,
//!     1_000_000_000, // 1 SOL
//!     /* swap_for_y = */ true,
//!     bin_arrays,
//!     None,
//!     /* current_timestamp = */ 1_700_000_000,
//!     /* current_slot     = */ 0,
//! )?;
//! ```

pub mod bin;
pub mod bin_array;
pub mod bitmap;
pub mod lb_pair;
pub mod price;
pub mod quote;
pub mod safe_math;
pub mod u128x128;
pub mod u64x64;
pub mod util;

pub use price::get_price_from_id;
pub use quote::{
    get_bin_array_pubkeys_for_swap, quote_exact_in, quote_exact_out, ExactInQuote, ExactOutQuote,
};
