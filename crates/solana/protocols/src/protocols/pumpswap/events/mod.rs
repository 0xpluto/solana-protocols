//! PumpSwap program events (log parsing).
//!
//! PumpSwap emits `BuyEvent` and `SellEvent` via Anchor's
//! `emit_cpi!` macro — the payload arrives as an inner self-CPI ix
//! whose data is `[ANCHOR_EVENT_DISCRIMINATOR (8) || event-name disc
//! (8) || borsh body]`. The legacy `emit!` path (program-data logs)
//! is also supported as a fallback for older program versions.
//!
//! Either kind contains everything an extractor needs to populate
//! a [`Swap`](crate::chain::Swap) — executed amounts, post-trade
//! pool reserves, fee components, and the coin creator. No CPI-
//! transfer reconciliation required.

mod buy;
mod collect_coin_creator_fee;
mod sell;

pub use buy::{BuyEvent, BUY_EVENT_DISCRIMINATOR};
pub use sell::{SellEvent, SELL_EVENT_DISCRIMINATOR};

pub use collect_coin_creator_fee::{
    CollectCoinCreatorFeeEvent, COLLECT_COIN_CREATOR_FEE_EVENT_DISCRIMINATOR,
};

/// Every event this module decodes; see pumpfun's `DECODED_EVENTS` for why the
/// names come from the impls.
///
/// `BuyEvent` and `SellEvent` are string literals because they are still
/// hand-counted byte offsets and do not implement `ProtocolEvent`. That
/// asymmetry is the reminder: converting them is the outstanding work here.
pub const DECODED_EVENTS: &[&str] = &[
    "BuyEvent",
    "SellEvent",
    <CollectCoinCreatorFeeEvent as crate::parsing::event::ProtocolEvent>::NAME,
];
