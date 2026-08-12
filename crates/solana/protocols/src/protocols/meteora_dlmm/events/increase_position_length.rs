//! `IncreasePositionLength` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::IncreasePositionLength as IncreasePositionLengthEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:IncreasePositionLength` discriminator. Source of truth: DLMM IDL.
pub const INCREASE_POSITION_LENGTH_EVENT_DISCRIMINATOR: [u8; 8] = [157, 239, 42, 204, 30, 56, 223, 46];

/// Decode the borsh body of a `IncreasePositionLength` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<IncreasePositionLengthEvent, InstructionParseError> {
    IncreasePositionLengthEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "increase_position_length event borsh decode: {e}"
        ))
    })
}
