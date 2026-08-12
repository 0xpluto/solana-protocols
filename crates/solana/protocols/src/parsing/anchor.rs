//! Anchor wire conventions shared by every Anchor program.
//!
//! Nothing here is protocol-specific — the name-strip test passes on every
//! item. It lives at this altitude because four protocols consume it
//! (pumpfun, pumpswap, Meteora DLMM, Meteora DAMM v2) and it previously sat
//! under `meteora_damm_v2::constants`, which three of those four imported
//! across a protocol boundary for a fact that is not Meteora's.

/// The tag prefixing every `emit_cpi!` event instruction, on any Anchor
/// program.
///
/// Anchor's `emit_cpi!` publishes an event as a **self-CPI** whose instruction
/// data is:
///
/// ```text
/// [ 0.. 8]  ANCHOR_EVENT_TAG          — this constant, identical everywhere
/// [ 8..16]  sha256("event:<Name>")[..8] — which event
/// [16..  ]  borsh-serialised body
/// ```
///
/// **It is a magic constant, not a hash.** It is Anchor's `EVENT_IX_TAG`
/// (`0x1d9acb512ea545e4`) in little-endian byte order. A doc comment on the
/// previous definition claimed `sha256("anchor:event")[..8]`; that is false and
/// was checked — no plausible preimage produces these bytes.
///
/// Because it is identical on every program and every event, matching it alone
/// answers *"is this an Anchor event?"* and never *"is this **my** event?"*.
/// The event identity lives at `[8..16]` and must be checked separately —
/// skipping that check is what let pumpfun accept any sufficiently long Anchor
/// event as a `TradeEvent`.
pub const ANCHOR_EVENT_TAG: [u8; 8] = [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d];

/// Split an `emit_cpi!` event instruction into its event discriminator and
/// body, or `None` if the data does not carry the Anchor event tag.
///
/// Returning the discriminator rather than taking an expected one keeps this
/// usable both for dispatch (which event is this?) and for a targeted match.
#[must_use]
pub fn split_event_ix(data: &[u8]) -> Option<(&[u8; 8], &[u8])> {
    if data.len() < 16 || data[..8] != ANCHOR_EVENT_TAG {
        return None;
    }
    let disc: &[u8; 8] = data[8..16].try_into().ok()?;
    Some((disc, &data[16..]))
}

/// The body of an `emit_cpi!` event instruction, but only when it carries
/// `expected` as its event discriminator.
#[must_use]
pub fn event_body<'a>(data: &'a [u8], expected: &[u8; 8]) -> Option<&'a [u8]> {
    split_event_ix(data)
        .filter(|(disc, _)| *disc == expected)
        .map(|(_, body)| body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tag identifies "an Anchor event", never "which event" — the
    /// distinction the pumpfun bug collapsed.
    #[test]
    fn the_tag_alone_does_not_identify_an_event() {
        let mine = [1u8; 8];
        let theirs = [2u8; 8];

        let mut data = ANCHOR_EVENT_TAG.to_vec();
        data.extend_from_slice(&theirs);
        data.extend_from_slice(&[0xAB; 40]);

        // Tag present, so it *is* an Anchor event...
        assert!(split_event_ix(&data).is_some());
        // ...but not one of ours, and asking for ours must say so.
        assert!(event_body(&data, &mine).is_none());
        assert_eq!(event_body(&data, &theirs), Some(&[0xAB; 40][..]));
    }

    #[test]
    fn non_event_data_and_truncated_data_are_refused() {
        assert!(split_event_ix(&[]).is_none());
        assert!(
            split_event_ix(&ANCHOR_EVENT_TAG).is_none(),
            "tag with no disc"
        );

        let mut wrong_tag = vec![0u8; 8];
        wrong_tag.extend_from_slice(&[0u8; 8]);
        assert!(split_event_ix(&wrong_tag).is_none());
    }

    /// An empty body is a real outcome (an event with no fields), distinct
    /// from "not an event" — so it must be `Some(&[])`, never `None`.
    #[test]
    fn an_empty_body_is_present_not_absent() {
        let disc = [7u8; 8];
        let mut data = ANCHOR_EVENT_TAG.to_vec();
        data.extend_from_slice(&disc);
        assert_eq!(event_body(&data, &disc), Some(&[][..]));
    }
}
