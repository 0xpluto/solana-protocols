//! PumpSwap `SellEvent` - borsh over the program's own field list.
//!
//! # Why this is not a hand-counted offset table
//!
//! It was, until 2026-08-15, and the table had 23 of the 27 fields the
//! program emits: everything past `coin_creator_fee` was invisible, including
//! the fee split this codebase later needed. Converting it is also how the
//! offsets stop being something a person maintains.
//!
//! The first attempt at this conversion **took pumpswap swaps to zero** and was
//! reverted. Its unit tests passed because they serialized a synthetic body with
//! the same struct that decoded it, which proves only that the struct agrees
//! with itself. The layout below is instead pinned by real mainnet bodies under
//! `fixtures/pumpswap/`, captured from the firehose.
//!
//! # The trailing bytes
//!
//! Measured against those bodies, **both the vendored and the live on-chain IDL
//! are behind the program by 25 bytes**. borsh refuses trailing bytes, so a
//! struct faithful to either IDL fails on every real body - which is exactly how
//! the first attempt broke. [`UndeclaredTail`] is the seam: as the final field it
//! consumes the remainder, so strict decoding succeeds and the bytes are kept
//! rather than guessed at or dropped.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::{ProtocolEvent, UndeclaredTail};

/// `sha256("event:SellEvent")[..8]`, derived at compile time.
pub const SELL_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("SellEvent");

/// A completed PumpSwap sell: base in, quote out, and the fee split.
///
/// Field list generated from the live on-chain IDL, not transcribed.
#[derive(Debug, Clone, Default, PartialEq, Eq, borsh::BorshDeserialize, borsh::BorshSerialize)]
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
    /// Bytes the program emits past every field its IDL declares.
    ///
    /// Non-empty today - 25 bytes, all zero in every sample captured so far -
    /// because the program is ahead of its published interface. Kept so the
    /// next person can identify them rather than rediscover that they exist.
    pub undeclared_tail: UndeclaredTail,
}

impl ProtocolEvent for SellEvent {
    const DISCRIMINATOR: [u8; 8] = SELL_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "SellEvent";
}
