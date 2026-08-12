//! `AddLiquidity` event wrapper.
//!
//! Generated. Re-run `tools/gen_dlmm_events.py` after an SDK / IDL
//! upgrade — this file isn't hand-edited.

pub use meteora_dlmm_sdk::types::AddLiquidity as AddLiquidityEvent;

use borsh::BorshDeserialize;

use crate::parsing::InstructionParseError;

/// `event:AddLiquidity` discriminator. Source of truth: DLMM IDL.
pub const ADD_LIQUIDITY_EVENT_DISCRIMINATOR: [u8; 8] = [31, 94, 125, 90, 227, 52, 61, 186];

/// Decode the borsh body of a `AddLiquidity` event. The caller is
/// responsible for having already stripped the 16 bytes of
/// envelope (anchor event disc + per-event disc) — see
/// [`super::parse_event_body`].
pub fn decode(body: &[u8]) -> Result<AddLiquidityEvent, InstructionParseError> {
    AddLiquidityEvent::try_from_slice(body).map_err(|e| {
        InstructionParseError::DeserializationFailed(format!(
            "add_liquidity event borsh decode: {e}"
        ))
    })
}
