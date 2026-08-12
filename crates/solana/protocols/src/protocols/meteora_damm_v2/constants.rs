//! Meteora DAMM v2 (cp-amm) on-chain constants.

use solana_program::pubkey::Pubkey;

/// Program ID: `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");

/// Fee denominator: `1_000_000_000` (10^9 = 100%).
pub const FEE_DENOMINATOR: u128 = 1_000_000_000;

/// `swap` instruction discriminator. `SHA256("global:swap")[..8]` — the
/// same bytes DAMM v1 uses because both Anchor programs chose the same
/// instruction name.
pub const SWAP_DISCRIMINATOR: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];

/// Generic Anchor `emit_cpi!` envelope discriminator (the *outer* 8 bytes
/// of every self-CPI event's `ix.data`). Equals
/// Re-export of the shared Anchor event tag under its historical name here.
///
/// The value is not Meteora's and not a hash — see
/// [`ANCHOR_EVENT_TAG`](crate::parsing::anchor::ANCHOR_EVENT_TAG), which now
/// owns it. Three other protocols imported it from this module across a
/// protocol boundary; the definition moved, this name stays for in-crate
/// callers.
pub use crate::parsing::anchor::ANCHOR_EVENT_TAG as ANCHOR_EVENT_DISCRIMINATOR;

/// `EvtSwap2` event-specific discriminator. `SHA256("event:EvtSwap2")[..8]`.
/// Follows [`ANCHOR_EVENT_DISCRIMINATOR`] in the self-CPI payload.
pub const EVT_SWAP2_DISCRIMINATOR: [u8; 8] = [0xbd, 0x42, 0x33, 0xa8, 0x26, 0x50, 0x75, 0x99];
