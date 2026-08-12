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

mod trade;

pub use trade::{TradeEvent, TradeFees, TRADE_EVENT_DISCRIMINATOR};
