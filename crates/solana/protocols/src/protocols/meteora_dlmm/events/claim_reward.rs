//! `ClaimReward` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::ClaimReward as ClaimRewardEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:ClaimReward` discriminator. Source of truth: DLMM IDL.
pub const CLAIM_REWARD_EVENT_DISCRIMINATOR: [u8; 8] = [148, 116, 134, 204, 22, 171, 85, 95];

/// Decode the borsh body of a `ClaimReward` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<ClaimRewardEvent, InstructionParseError> {
    ClaimRewardEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "claim_reward event borsh decode: {e}"
        ))
    })
}
