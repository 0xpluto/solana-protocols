//! Anchor `emit_cpi!` event parsers for Meteora DLMM.
//!
//! DLMM does **not** use the legacy `emit!` (program-data log) path.
//! Every event is delivered as an inner self-CPI instruction whose
//! `data` is shaped:
//!
//! ```text
//! [ 0..  8] ANCHOR_EVENT_DISCRIMINATOR  (constant for all Anchor programs)
//! [ 8.. 16] <event-name discriminator>  (sha256("event:<Name>")[..8])
//! [16..   ] borsh-serialised event body (matches the SDK's `types::<Name>` struct)
//! ```
//!
//! The dispatcher in `_dispatch.rs` matches the per-event byte
//! sequence and routes to the right per-event `decode()`. Per-event
//! files re-export the SDK's borsh-derived struct as `<Name>Event` so
//! consumers don't have to reach into the SDK's `types/` namespace.
//!
//! ## Walking from the outer instruction
//!
//! The extractor (step 5) holds the outer ix and the full instruction
//! list. It walks children with `parent_index == outer.instruction_index`
//! looking for `program_id == DLMM_PROGRAM`, calls
//! [`parse_event_body`] on each, and uses whichever event variant it
//! cares about. `find_event_with_disc` is the convenience that
//! short-circuits to a single expected variant.
//!
//! Files in this directory are emitted by `tools/gen_dlmm_events.py`
//! (per-event wrappers + the `_dispatch.rs` include). Re-run after an
//! IDL upgrade — none of this is hand-edited beyond `mod.rs` itself.

use crate::parsing::{InstructionParseError, ParsedInstruction};

use super::PROGRAM_ID;

/// Anchor `emit_cpi!` envelope marker — the first 8 bytes of every
/// inner self-CPI event ix. Re-exported so external crates (the
/// `lp-position` tx ingestor, replay harnesses) can identify event
/// envelopes without reaching across protocol modules.
pub const ANCHOR_EVENT_DISCRIMINATOR: [u8; 8] =
    crate::protocols::meteora_damm_v2::constants::ANCHOR_EVENT_DISCRIMINATOR;

include!("_dispatch.rs");

/// Decode an inner-CPI ix's `data` slice into a typed event.
///
/// Strips the 8-byte anchor envelope + 8-byte per-event discriminator
/// before borsh-decoding the payload. Returns
/// [`InstructionParseError::UnknownDiscriminator`] if the per-event
/// disc doesn't match any registered DLMM event;
/// [`InstructionParseError::DataTooShortDetailed`] if the data is
/// shorter than the 16-byte envelope; and any borsh failure surfaces
/// as `DeserializationFailed`.
///
/// Caller is expected to have already verified
/// `ix.program_id == PROGRAM_ID` and the anchor envelope's first 8
/// bytes — but we re-check both so this is safe to call on arbitrary
/// inner ix slices.
pub fn parse_event_body(data: &[u8]) -> Result<MeteoraDlmmEvent, InstructionParseError> {
    if data.len() < 16 {
        return Err(InstructionParseError::DataTooShortDetailed {
            expected: 16,
            actual: data.len(),
        });
    }
    if data[..8] != ANCHOR_EVENT_DISCRIMINATOR {
        // Not an Anchor event ix — caller passed something else.
        return Err(InstructionParseError::UnknownDiscriminator(
            data[..8].try_into().expect("8-byte slice"),
        ));
    }
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&data[8..16]);
    dispatch(&disc, &data[16..])
}

/// Walk children of `outer_ix` and return every DLMM event emitted
/// inside it. Convenient for handlers that want the full event
/// stream of one swap or liquidity op (which can fan out to
/// `Swap` + `CompositionFee`, `AddLiquidity` + `PositionCreate`, etc.).
pub fn walk_events(
    outer_ix: &ParsedInstruction,
    all: &[ParsedInstruction],
) -> Vec<MeteoraDlmmEvent> {
    let outer_idx = outer_ix.instruction_index;
    let mut out = Vec::new();
    for child in all.iter() {
        if child.parent_index != Some(outer_idx) {
            continue;
        }
        if child.program_id != PROGRAM_ID {
            continue;
        }
        if child.data.len() < 16 || child.data[..8] != ANCHOR_EVENT_DISCRIMINATOR {
            continue;
        }
        if let Ok(ev) = parse_event_body(&child.data) {
            out.push(ev);
        }
    }
    out
}

/// Locate the *first* child of `outer_ix` whose per-event
/// discriminator equals `expected_disc`, and return the borsh body
/// (no envelope). Used by the extractor when it knows exactly which
/// event variant the outer ix should produce (e.g. `Swap` ix → look
/// for `EVT_SWAP`).
pub fn find_event_body<'a>(
    outer_ix: &ParsedInstruction,
    all: &'a [ParsedInstruction],
    expected_disc: &[u8; 8],
) -> Option<&'a [u8]> {
    let outer_idx = outer_ix.instruction_index;
    for child in all.iter() {
        if child.parent_index != Some(outer_idx) {
            continue;
        }
        if child.program_id != PROGRAM_ID {
            continue;
        }
        if child.data.len() < 16 {
            continue;
        }
        if child.data[..8] != ANCHOR_EVENT_DISCRIMINATOR {
            continue;
        }
        if &child.data[8..16] != expected_disc {
            continue;
        }
        return Some(&child.data[16..]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::ParsedInstructionBuilder;
    use borsh::BorshSerialize;
    use solana_program::pubkey::Pubkey;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    /// Build a fake inner-CPI ix carrying a borsh-serialised event.
    fn build_event_ix<T: BorshSerialize>(
        parent_idx: usize,
        instruction_idx: usize,
        event_disc: [u8; 8],
        payload: &T,
    ) -> ParsedInstruction {
        let mut data = ANCHOR_EVENT_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&event_disc);
        data.extend(borsh::to_vec(payload).expect("borsh"));
        ParsedInstructionBuilder::new()
            .program_id(PROGRAM_ID)
            .accounts(vec![pk(0)])
            .data(data)
            .instruction_index(instruction_idx)
            .parent_index(parent_idx)
            .build()
    }

    #[test]
    fn parse_event_body_rejects_short_data() {
        assert!(matches!(
            parse_event_body(&[0u8; 4]),
            Err(InstructionParseError::DataTooShortDetailed { .. })
        ));
    }

    #[test]
    fn parse_event_body_rejects_wrong_anchor_envelope() {
        let mut bad = vec![0xAA; 8];
        bad.extend(swap::SWAP_EVENT_DISCRIMINATOR);
        bad.extend(vec![0u8; 100]);
        assert!(matches!(
            parse_event_body(&bad),
            Err(InstructionParseError::UnknownDiscriminator(_))
        ));
    }

    #[test]
    fn walk_finds_inner_swap_event() {
        let payload = swap::SwapEvent {
            lb_pair: pk(1).to_bytes().into(),
            from: pk(2).to_bytes().into(),
            start_bin_id: -10,
            end_bin_id: -5,
            amount_in: 1_000_000,
            amount_out: 950_000,
            swap_for_y: true,
            fee: 5_000,
            protocol_fee: 1_000,
            fee_bps: 30,
            host_fee: 0,
        };
        let outer = ParsedInstructionBuilder::new()
            .program_id(PROGRAM_ID)
            .accounts(vec![pk(0); 15])
            .data(vec![0u8; 24])
            .instruction_index(0)
            .build();
        let inner = build_event_ix(0, 1, swap::SWAP_EVENT_DISCRIMINATOR, &payload);
        let events = walk_events(&outer, &[outer.clone(), inner]);
        assert_eq!(events.len(), 1);
        match &events[0] {
            MeteoraDlmmEvent::Swap(s) => {
                assert_eq!(s.amount_in, 1_000_000);
                assert!(s.swap_for_y);
            }
            other => panic!("expected Swap, got {other:?}"),
        }
    }
}
