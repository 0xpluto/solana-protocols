//! `RemoveLiquidity` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::RemoveLiquidity as RemoveLiquidityEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:RemoveLiquidity` discriminator. Source of truth: DLMM IDL.
pub const REMOVE_LIQUIDITY_EVENT_DISCRIMINATOR: [u8; 8] = [116, 244, 97, 232, 103, 31, 152, 58];

/// Decode the borsh body of a `RemoveLiquidity` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<RemoveLiquidityEvent, InstructionParseError> {
    RemoveLiquidityEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "remove_liquidity event borsh decode: {e}"
        ))
    })
}
