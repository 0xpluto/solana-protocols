//! `IncreaseObservation` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::IncreaseObservation as IncreaseObservationEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:IncreaseObservation` discriminator. Source of truth: DLMM IDL.
pub const INCREASE_OBSERVATION_EVENT_DISCRIMINATOR: [u8; 8] = [99, 249, 17, 121, 166, 156, 207, 215];

/// Decode the borsh body of a `IncreaseObservation` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<IncreaseObservationEvent, InstructionParseError> {
    IncreaseObservationEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "increase_observation event borsh decode: {e}"
        ))
    })
}
