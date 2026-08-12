//! `LbPairCreate` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::LbPairCreate as LbPairCreateEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:LbPairCreate` discriminator. Source of truth: DLMM IDL.
pub const LB_PAIR_CREATE_EVENT_DISCRIMINATOR: [u8; 8] = [185, 74, 252, 125, 27, 215, 188, 111];

/// Decode the borsh body of a `LbPairCreate` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<LbPairCreateEvent, InstructionParseError> {
    LbPairCreateEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "lb_pair_create event borsh decode: {e}"
        ))
    })
}
