//! `ClaimFee` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::ClaimFee as ClaimFeeEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:ClaimFee` discriminator. Source of truth: DLMM IDL.
pub const CLAIM_FEE_EVENT_DISCRIMINATOR: [u8; 8] = [75, 122, 154, 48, 140, 74, 123, 163];

/// Decode the borsh body of a `ClaimFee` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<ClaimFeeEvent, InstructionParseError> {
    ClaimFeeEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "claim_fee event borsh decode: {e}"
        ))
    })
}
