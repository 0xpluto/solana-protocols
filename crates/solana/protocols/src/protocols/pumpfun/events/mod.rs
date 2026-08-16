//! Pump.fun program events (log parsing).
//!
//! This module provides parsing for pump.fun program logs, allowing you to
//! extract trade events and other data from transaction logs.
//!
//! # Usage
//!
//! ```ignore
//! use solana_protocols::pumpfun::events::TradeEvent;
//!
//! // Parse from log lines
//! for log in transaction_logs {
//!     if let Some(Ok(event)) = TradeEvent::try_from_log(&log) {
//!         println!("Trade: {} {} tokens for {} SOL",
//!             if event.is_buy { "bought" } else { "sold" },
//!             event.token_amount_ui(),
//!             event.sol_amount_ui()
//!         );
//!     }
//! }
//! ```

mod creator_fee;
mod trade;

pub use creator_fee::{
    CollectCreatorFeeEvent, DistributeCreatorFeesEvent, COLLECT_CREATOR_FEE_EVENT_DISCRIMINATOR,
    DISTRIBUTE_CREATOR_FEES_EVENT_DISCRIMINATOR,
};
pub use trade::{Shareholder, TradeEvent, TRADE_EVENT_DISCRIMINATOR};

/// Every event this module decodes, named by the impls themselves.
///
/// Read by the parse-coverage meter. Sourced from
/// [`ProtocolEvent::NAME`](crate::parsing::event::ProtocolEvent::NAME) rather
/// than typed out, so the coverage number cannot drift from the code by a
/// typo — the one remaining way to be wrong is forgetting to add a line here,
/// which understates coverage rather than overstating it.
pub const DECODED_EVENTS: &[&str] = &[
    <TradeEvent as crate::parsing::event::ProtocolEvent>::NAME,
    <CollectCreatorFeeEvent as crate::parsing::event::ProtocolEvent>::NAME,
    <DistributeCreatorFeesEvent as crate::parsing::event::ProtocolEvent>::NAME,
];
