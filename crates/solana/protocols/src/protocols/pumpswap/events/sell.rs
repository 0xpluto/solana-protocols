//! PumpSwap `SellEvent` - borsh over the program's own field list.
//!
//! # Why this is not a hand-counted offset table
//!
//! It was, until 2026-08-15, and it read 23 fields of the 27 the program
//! emits. Everything past `coin_creator_fee` was invisible, including the fee
//! split this codebase later went looking for.
//!
//! The first attempt at this conversion **took pumpswap swaps to zero** and was
//! reverted. It shipped green because its tests serialized a synthetic body with
//! the same struct that decoded it, which proves the struct agrees with itself
//! and nothing else. The layout below is pinned by real mainnet bodies under
//! `fixtures/pumpswap/event_bodies/`, and by `#[derive(EventLayout)]`, which
//! fails the build if the field list stops matching the vendored IDL.
//!
//! # Fields the IDL does not declare
//!
//! The program runs ahead of its published interface: real bodies carry 25
//! bytes past the last field the IDL declares. borsh refuses trailing bytes, so
//! a struct faithful to the IDL fails on every real body - that is precisely
//! what broke the first attempt. Those fields are modelled and marked
//! `#[idl(undeclared = "...")]`, which exempts them from the layout check while
//! keeping the data.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::EventLayout;

use crate::parsing::event::{ProtocolEvent, UndeclaredTail};

/// `sha256("event:SellEvent")[..8]`, derived at compile time.
pub const SELL_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("SellEvent");

/// A completed PumpSwap sell: base in, quote out, and the full fee split.
///
/// Field list generated from the on-chain IDL, not transcribed, and verified
/// against it at compile time.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    EventLayout,
)]
#[idl(program = "pump_amm", event = "SellEvent")]
pub struct SellEvent {
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `base_amount_in` — declared by the program IDL.
    pub base_amount_in: u64,
    /// `min_quote_amount_out` — declared by the program IDL.
    pub min_quote_amount_out: u64,
    /// `user_base_token_reserves` — declared by the program IDL.
    pub user_base_token_reserves: u64,
    /// `user_quote_token_reserves` — declared by the program IDL.
    pub user_quote_token_reserves: u64,
    /// `pool_base_token_reserves` — declared by the program IDL.
    pub pool_base_token_reserves: u64,
    /// `pool_quote_token_reserves` — declared by the program IDL.
    pub pool_quote_token_reserves: u64,
    /// `quote_amount_out` — declared by the program IDL.
    pub quote_amount_out: u64,
    /// `lp_fee_basis_points` — declared by the program IDL.
    pub lp_fee_basis_points: u64,
    /// `lp_fee` — declared by the program IDL.
    pub lp_fee: u64,
    /// `protocol_fee_basis_points` — declared by the program IDL.
    pub protocol_fee_basis_points: u64,
    /// `protocol_fee` — declared by the program IDL.
    pub protocol_fee: u64,
    /// `quote_amount_out_without_lp_fee` — declared by the program IDL.
    pub quote_amount_out_without_lp_fee: u64,
    /// `user_quote_amount_out` — declared by the program IDL.
    pub user_quote_amount_out: u64,
    /// `pool` — declared by the program IDL.
    pub pool: Pubkey,
    /// `user` — declared by the program IDL.
    pub user: Pubkey,
    /// `user_base_token_account` — declared by the program IDL.
    pub user_base_token_account: Pubkey,
    /// `user_quote_token_account` — declared by the program IDL.
    pub user_quote_token_account: Pubkey,
    /// `protocol_fee_recipient` — declared by the program IDL.
    pub protocol_fee_recipient: Pubkey,
    /// `protocol_fee_recipient_token_account` — declared by the program IDL.
    pub protocol_fee_recipient_token_account: Pubkey,
    /// `coin_creator` — declared by the program IDL.
    pub coin_creator: Pubkey,
    /// `coin_creator_fee_basis_points` — declared by the program IDL.
    pub coin_creator_fee_basis_points: u64,
    /// `coin_creator_fee` — declared by the program IDL.
    pub coin_creator_fee: u64,
    /// `cashback_fee_basis_points` — declared by the program IDL.
    pub cashback_fee_basis_points: u64,
    /// `cashback` — declared by the program IDL.
    pub cashback: u64,
    /// `buyback_fee_basis_points` — declared by the program IDL.
    pub buyback_fee_basis_points: u64,
    /// `buyback_fee` — declared by the program IDL.
    pub buyback_fee: u64,

    // ── Past the IDL ────────────────────────────────────────────────────────
    // The program emits 25 bytes more than it declares, on both trade events.
    // They are named here rather than held as a blob because a typed field is
    // checkable: `undeclared_flag` being a `bool` means borsh refuses any byte
    // outside {0, 1}, so if the split below is wrong the very next body says so
    // instead of yielding plausible garbage. The split is inferred from three
    // captured bodies (8 + 8 + 1 + 8 = 25) and is the weakest claim on this
    // page.
    /// Undeclared. Reads ~1.75e10 on both captured buys, zero on the sell.
    #[idl(undeclared = "unknown")]
    pub undeclared_0: u64,
    /// Undeclared. Zero in every body captured so far.
    #[idl(undeclared = "unknown")]
    pub undeclared_1: u64,
    /// Undeclared. Reads 1 on both captured buys and 0 on the sell, which is
    /// what makes `bool` the honest type and the enforcing one.
    #[idl(undeclared = "unknown")]
    pub undeclared_flag: bool,
    /// Undeclared. Non-zero on buys and moves with the trade; magnitude ~1e15
    /// suggests a cumulative counter rather than a per-trade amount.
    #[idl(undeclared = "unknown")]
    pub undeclared_2: u64,
    /// Anything past even the undeclared fields above.
    ///
    /// **Empty today**, and pinned that way by a fixture test. It exists so the
    /// next time this program grows a field, decoding keeps working and the
    /// length tells us how much it grew — rather than borsh rejecting every
    /// body, which is exactly how the first attempt at this conversion took
    /// pumpswap swaps to zero.
    #[idl(undeclared = "growth headroom; see UndeclaredTail")]
    pub undeclared_tail: UndeclaredTail,
}

impl ProtocolEvent for SellEvent {
    const DISCRIMINATOR: [u8; 8] = SELL_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "SellEvent";
}
