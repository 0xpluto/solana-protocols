//! `GoToABin` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::GoToABin as GoToABinEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:GoToABin` discriminator. Source of truth: DLMM IDL.
pub const GO_TO_A_BIN_EVENT_DISCRIMINATOR: [u8; 8] = [59, 138, 76, 68, 138, 131, 176, 67];

/// Decode the borsh body of a `GoToABin` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<GoToABinEvent, InstructionParseError> {
    GoToABinEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "go_to_a_bin event borsh decode: {e}"
        ))
    })
}
