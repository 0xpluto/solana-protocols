//! `ClaimReward2` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::ClaimReward2 as ClaimReward2Event;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:ClaimReward2` discriminator. Source of truth: DLMM IDL.
pub const CLAIM_REWARD2_EVENT_DISCRIMINATOR: [u8; 8] = [27, 143, 244, 33, 80, 43, 110, 146];

/// Decode the borsh body of a `ClaimReward2` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<ClaimReward2Event, InstructionParseError> {
    ClaimReward2Event::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "claim_reward2 event borsh decode: {e}"
        ))
    })
}
