//! `Rebalancing` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::Rebalancing as RebalancingEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:Rebalancing` discriminator. Source of truth: DLMM IDL.
pub const REBALANCING_EVENT_DISCRIMINATOR: [u8; 8] = [0, 109, 117, 179, 61, 91, 199, 200];

/// Decode the borsh body of a `Rebalancing` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<RebalancingEvent, InstructionParseError> {
    RebalancingEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "rebalancing event borsh decode: {e}"
        ))
    })
}
