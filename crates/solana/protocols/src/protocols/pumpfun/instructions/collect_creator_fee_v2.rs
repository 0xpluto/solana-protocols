//! Pump.fun `collect_creator_fee_v2`.
//!
//! The same withdrawal, settling into a token account rather than a bare lamport transfer.
//!
//! Zero arguments, so the instruction data carries no economics — everything
//! worth recording is in the event it emits. [`NoParams`] refuses a non-empty
//! body rather than ignoring it: trailing bytes here would mean the program
//! grew an argument, which is exactly the change that must announce itself.
//!
//! # Accounts
//!
//! Deliberately no account struct. The IDL declares 10; mainnet has been
//! observed sending more, the same drift the v2 swap instructions show. A
//! fixed-slot struct would decode the wrong pubkeys the moment the program adds
//! one, and the event names its own participants anyway.

pub use crate::parsing::NoParams as CollectCreatorFeeV2Params;
