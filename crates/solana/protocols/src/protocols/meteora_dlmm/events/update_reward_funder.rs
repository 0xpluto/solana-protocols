//! `UpdateRewardFunder` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::UpdateRewardFunder as UpdateRewardFunderEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:UpdateRewardFunder` discriminator. Source of truth: DLMM IDL.
pub const UPDATE_REWARD_FUNDER_EVENT_DISCRIMINATOR: [u8; 8] = [224, 178, 174, 74, 252, 165, 85, 180];

/// Decode the borsh body of a `UpdateRewardFunder` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<UpdateRewardFunderEvent, InstructionParseError> {
    UpdateRewardFunderEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "update_reward_funder event borsh decode: {e}"
        ))
    })
}
