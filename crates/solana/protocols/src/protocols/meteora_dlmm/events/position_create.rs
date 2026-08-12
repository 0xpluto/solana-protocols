//! `PositionCreate` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::PositionCreate as PositionCreateEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:PositionCreate` discriminator. Source of truth: DLMM IDL.
pub const POSITION_CREATE_EVENT_DISCRIMINATOR: [u8; 8] = [144, 142, 252, 84, 157, 53, 37, 121];

/// Decode the borsh body of a `PositionCreate` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<PositionCreateEvent, InstructionParseError> {
    PositionCreateEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "position_create event borsh decode: {e}"
        ))
    })
}
