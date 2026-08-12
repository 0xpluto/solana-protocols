//! Meteora DAMM v2 (cp-amm) protocol.
//!
//! Program ID: `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`.
//!
//! Concentrated-liquidity AMM. Tokens often graduate here from Meteora
//! DBC bonding curves.
//!
//! # Current scope
//!
//! Extraction only — we parse swap transactions into semantic
//! [`Swap`](crate::chain::Swap) events. Full trading support (pool state,
//! curve math, instruction building) is deferred until we wire execution.
//!
//! # Event flow
//!
//! DAMM v2 uses Anchor `emit_cpi!` for its swap event, not legacy `emit!`.
//! The event rides as an inner self-CPI under the outer `swap` ix, so
//! the extractor walks `parent_index` from the inner event ix up to the
//! outer swap to pull mint / trader accounts. See [`extract`].

pub mod constants;
pub mod events;
pub mod extract;
pub mod instructions;

pub use constants::{
    ANCHOR_EVENT_DISCRIMINATOR, EVT_SWAP2_DISCRIMINATOR, FEE_DENOMINATOR, PROGRAM_ID,
    SWAP_DISCRIMINATOR,
};
pub use events::{EvtSwap2, SwapParameters2, SwapResult2};
pub use extract::MeteoraDammV2Extractor;
pub use instructions::{SwapAccounts, SwapParams};
