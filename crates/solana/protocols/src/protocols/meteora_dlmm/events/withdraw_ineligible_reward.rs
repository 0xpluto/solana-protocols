//! `WithdrawIneligibleReward` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::WithdrawIneligibleReward as WithdrawIneligibleRewardEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:WithdrawIneligibleReward` discriminator. Source of truth: DLMM IDL.
pub const WITHDRAW_INELIGIBLE_REWARD_EVENT_DISCRIMINATOR: [u8; 8] = [231, 189, 65, 149, 102, 215, 154, 244];

/// Decode the borsh body of a `WithdrawIneligibleReward` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<WithdrawIneligibleRewardEvent, InstructionParseError> {
    WithdrawIneligibleRewardEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "withdraw_ineligible_reward event borsh decode: {e}"
        ))
    })
}
