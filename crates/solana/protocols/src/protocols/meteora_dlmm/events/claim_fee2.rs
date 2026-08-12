//! `ClaimFee2` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::ClaimFee2 as ClaimFee2Event;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:ClaimFee2` discriminator. Source of truth: DLMM IDL.
pub const CLAIM_FEE2_EVENT_DISCRIMINATOR: [u8; 8] = [232, 171, 242, 97, 58, 77, 35, 45];

/// Decode the borsh body of a `ClaimFee2` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<ClaimFee2Event, InstructionParseError> {
    ClaimFee2Event::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "claim_fee2 event borsh decode: {e}"
        ))
    })
}
