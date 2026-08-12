//! `DynamicFeeParameterUpdate` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::DynamicFeeParameterUpdate as DynamicFeeParameterUpdateEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:DynamicFeeParameterUpdate` discriminator. Source of truth: DLMM IDL.
pub const DYNAMIC_FEE_PARAMETER_UPDATE_EVENT_DISCRIMINATOR: [u8; 8] = [88, 88, 178, 135, 194, 146, 91, 243];

/// Decode the borsh body of a `DynamicFeeParameterUpdate` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<DynamicFeeParameterUpdateEvent, InstructionParseError> {
    DynamicFeeParameterUpdateEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "dynamic_fee_parameter_update event borsh decode: {e}"
        ))
    })
}
