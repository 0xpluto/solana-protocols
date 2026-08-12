//! `FundReward` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::FundReward as FundRewardEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:FundReward` discriminator. Source of truth: DLMM IDL.
pub const FUND_REWARD_EVENT_DISCRIMINATOR: [u8; 8] = [246, 228, 58, 130, 145, 170, 79, 204];

/// Decode the borsh body of a `FundReward` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<FundRewardEvent, InstructionParseError> {
    FundRewardEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "fund_reward event borsh decode: {e}"
        ))
    })
}
