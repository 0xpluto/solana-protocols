//! `CompositionFee` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::CompositionFee as CompositionFeeEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:CompositionFee` discriminator. Source of truth: DLMM IDL.
pub const COMPOSITION_FEE_EVENT_DISCRIMINATOR: [u8; 8] = [128, 151, 123, 106, 17, 102, 113, 142];

/// Decode the borsh body of a `CompositionFee` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<CompositionFeeEvent, InstructionParseError> {
    CompositionFeeEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "composition_fee event borsh decode: {e}"
        ))
    })
}
