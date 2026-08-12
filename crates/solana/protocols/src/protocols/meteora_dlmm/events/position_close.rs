//! `PositionClose` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::PositionClose as PositionCloseEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:PositionClose` discriminator. Source of truth: DLMM IDL.
pub const POSITION_CLOSE_EVENT_DISCRIMINATOR: [u8; 8] = [255, 196, 16, 107, 28, 202, 53, 128];

/// Decode the borsh body of a `PositionClose` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<PositionCloseEvent, InstructionParseError> {
    PositionCloseEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "position_close event borsh decode: {e}"
        ))
    })
}
