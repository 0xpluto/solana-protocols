//! `DecreasePositionLength` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::DecreasePositionLength as DecreasePositionLengthEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:DecreasePositionLength` discriminator. Source of truth: DLMM IDL.
pub const DECREASE_POSITION_LENGTH_EVENT_DISCRIMINATOR: [u8; 8] = [52, 118, 235, 85, 172, 169, 15, 128];

/// Decode the borsh body of a `DecreasePositionLength` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<DecreasePositionLengthEvent, InstructionParseError> {
    DecreasePositionLengthEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "decrease_position_length event borsh decode: {e}"
        ))
    })
}
