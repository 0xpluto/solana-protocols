//! `UpdateRewardDuration` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::UpdateRewardDuration as UpdateRewardDurationEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:UpdateRewardDuration` discriminator. Source of truth: DLMM IDL.
pub const UPDATE_REWARD_DURATION_EVENT_DISCRIMINATOR: [u8; 8] = [223, 245, 224, 153, 49, 29, 163, 172];

/// Decode the borsh body of a `UpdateRewardDuration` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<UpdateRewardDurationEvent, InstructionParseError> {
    UpdateRewardDurationEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "update_reward_duration event borsh decode: {e}"
        ))
    })
}
