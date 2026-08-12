//! `UpdatePositionLockReleasePoint` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::UpdatePositionLockReleasePoint as UpdatePositionLockReleasePointEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:UpdatePositionLockReleasePoint` discriminator. Source of truth: DLMM IDL.
pub const UPDATE_POSITION_LOCK_RELEASE_POINT_EVENT_DISCRIMINATOR: [u8; 8] = [133, 214, 66, 224, 64, 12, 7, 191];

/// Decode the borsh body of a `UpdatePositionLockReleasePoint` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<UpdatePositionLockReleasePointEvent, InstructionParseError> {
    UpdatePositionLockReleasePointEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "update_position_lock_release_point event borsh decode: {e}"
        ))
    })
}
