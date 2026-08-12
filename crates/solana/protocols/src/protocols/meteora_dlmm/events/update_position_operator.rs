//! `UpdatePositionOperator` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::UpdatePositionOperator as UpdatePositionOperatorEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:UpdatePositionOperator` discriminator. Source of truth: DLMM IDL.
pub const UPDATE_POSITION_OPERATOR_EVENT_DISCRIMINATOR: [u8; 8] = [39, 115, 48, 204, 246, 47, 66, 57];

/// Decode the borsh body of a `UpdatePositionOperator` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<UpdatePositionOperatorEvent, InstructionParseError> {
    UpdatePositionOperatorEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "update_position_operator event borsh decode: {e}"
        ))
    })
}
