//! `FeeParameterUpdate` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::FeeParameterUpdate as FeeParameterUpdateEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:FeeParameterUpdate` discriminator. Source of truth: DLMM IDL.
pub const FEE_PARAMETER_UPDATE_EVENT_DISCRIMINATOR: [u8; 8] = [48, 76, 241, 117, 144, 215, 242, 44];

/// Decode the borsh body of a `FeeParameterUpdate` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<FeeParameterUpdateEvent, InstructionParseError> {
    FeeParameterUpdateEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "fee_parameter_update event borsh decode: {e}"
        ))
    })
}
