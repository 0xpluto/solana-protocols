//! `InitializeReward` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::InitializeReward as InitializeRewardEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:InitializeReward` discriminator. Source of truth: DLMM IDL.
pub const INITIALIZE_REWARD_EVENT_DISCRIMINATOR: [u8; 8] = [211, 153, 88, 62, 149, 60, 177, 70];

/// Decode the borsh body of a `InitializeReward` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<InitializeRewardEvent, InstructionParseError> {
    InitializeRewardEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "initialize_reward event borsh decode: {e}"
        ))
    })
}
