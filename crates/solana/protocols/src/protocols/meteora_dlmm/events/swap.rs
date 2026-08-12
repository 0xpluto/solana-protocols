//! `Swap` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::Swap as SwapEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:Swap` discriminator. Source of truth: DLMM IDL.
pub const SWAP_EVENT_DISCRIMINATOR: [u8; 8] = [81, 108, 227, 190, 205, 208, 10, 196];

/// Decode the borsh body of a `Swap` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<SwapEvent, InstructionParseError> {
    SwapEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "swap event borsh decode: {e}"
        ))
    })
}
