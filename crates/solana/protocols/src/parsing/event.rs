//! `ProtocolEvent` — one contract for decoding an Anchor event.
//!
//! Events are where the executed truth lives. Pumpfun's fee split, PumpSwap's
//! reserves, and the layout-independent identity of a v2 swap all come from
//! the event rather than the instruction: the instruction says what someone
//! asked for, the event says what happened.
//!
//! Before this trait there was no contract at all. Three protocols spelled the
//! same operation three ways — `decode(body)` on 25 generated DLMM events,
//! `from_body(body)` on PumpSwap's two, and a free function on pumpfun's — with
//! nothing requiring a new event parser to pick any of them, and nothing
//! saying who strips the envelope.
//!
//! # The envelope is handled once
//!
//! An `emit_cpi!` event arrives as `[ANCHOR_EVENT_TAG | event disc | body]`.
//! Every protocol was re-implementing "check the tag, check the disc, decode
//! the rest", and pumpfun's copy skipped the disc check entirely — which is how
//! any sufficiently long Anchor event could be read as a trade. Here it is a
//! provided method, so getting it wrong requires overriding a default rather
//! than merely forgetting.
//!
//! # Strictness
//!
//! [`BorshDeserialize`] is a supertrait, and the body decode uses
//! `try_from_slice`, which refuses trailing bytes. That is deliberate and it is
//! the right side of the trade *here*, unlike on instruction arguments: an
//! event body is written by the program itself, not by an arbitrary sender, so
//! surplus bytes mean our layout is wrong rather than that someone appended
//! junk. Instruction params tolerate a remainder and record it as evidence;
//! event bodies do not get that latitude.

use borsh::BorshDeserialize;

use crate::parsing::anchor::event_body;
use crate::parsing::InstructionParseError;

/// An Anchor event that can be decoded from its on-chain representation.
pub trait ProtocolEvent: BorshDeserialize + Sized {
    /// `sha256("event:<Name>")[..8]` — the tag at bytes `[8..16]` of an
    /// `emit_cpi!` instruction, and at `[0..8]` of an `emit!` data log.
    const DISCRIMINATOR: [u8; 8];

    /// Human name, for telemetry and coverage reporting.
    const NAME: &'static str;

    /// Decode a bare body — no envelope, no discriminator.
    ///
    /// # Errors
    ///
    /// The bytes are not a valid borsh encoding of this event, or carry
    /// trailing bytes the layout does not account for.
    fn from_event_body(body: &[u8]) -> Result<Self, InstructionParseError> {
        Self::try_from_slice(body).map_err(|e| {
            InstructionParseError::DeserializationFailed(format!(
                "{}: borsh decode failed: {e}",
                Self::NAME
            ))
        })
    }

    /// Decode from a full `emit_cpi!` instruction: tag, discriminator, body.
    ///
    /// Returns `Ok(None)` when the data is a *different* event — that is
    /// routine dispatch, not a failure, and conflating the two is what makes a
    /// parse-error rate meaningless.
    ///
    /// # Errors
    ///
    /// The data carries this event's discriminator but its body does not
    /// decode — which is a real defect in our layout, not a foreign event.
    fn from_event_instruction(data: &[u8]) -> Result<Option<Self>, InstructionParseError> {
        match event_body(data, &Self::DISCRIMINATOR) {
            Some(body) => Self::from_event_body(body).map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(BorshDeserialize, Debug, PartialEq, Eq)]
    struct Tiny {
        a: u64,
    }
    impl ProtocolEvent for Tiny {
        const DISCRIMINATOR: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        const NAME: &'static str = "Tiny";
    }

    fn framed(disc: [u8; 8], body: &[u8]) -> Vec<u8> {
        let mut v = crate::parsing::anchor::ANCHOR_EVENT_TAG.to_vec();
        v.extend_from_slice(&disc);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn a_matching_event_decodes_through_the_envelope() {
        let data = framed(Tiny::DISCRIMINATOR, &7u64.to_le_bytes());
        assert_eq!(
            Tiny::from_event_instruction(&data).unwrap(),
            Some(Tiny { a: 7 })
        );
    }

    /// A different event is `Ok(None)`, not an error. Dispatch and failure are
    /// different facts and a rate that mixes them means nothing.
    #[test]
    fn a_foreign_event_is_not_an_error() {
        let data = framed([9; 8], &7u64.to_le_bytes());
        assert_eq!(Tiny::from_event_instruction(&data).unwrap(), None);
    }

    /// Our discriminator with a body we cannot decode IS an error: the program
    /// wrote this and we disagree about its shape.
    #[test]
    fn our_own_event_with_an_undecodable_body_is_an_error() {
        let data = framed(Tiny::DISCRIMINATOR, &[1, 2]);
        assert!(Tiny::from_event_instruction(&data).is_err());
    }

    /// Trailing bytes are refused. An event body is written by the program,
    /// not an arbitrary sender, so surplus bytes mean our layout is wrong.
    #[test]
    fn trailing_bytes_in_an_event_body_are_refused() {
        let mut body = 7u64.to_le_bytes().to_vec();
        body.push(0);
        assert!(Tiny::from_event_body(&body).is_err());
    }
}

#[cfg(test)]
mod real_events {
    use super::ProtocolEvent;
    use crate::protocols::meteora_dlmm::events::swap::SwapEvent;

    /// The trait's discriminator must equal the one the protocol declares —
    /// two constants that could drift, pinned to each other.
    #[test]
    fn a_real_event_carries_its_protocols_discriminator() {
        assert_eq!(
            <SwapEvent as ProtocolEvent>::DISCRIMINATOR,
            crate::protocols::meteora_dlmm::events::swap::SWAP_EVENT_DISCRIMINATOR
        );
        assert_eq!(<SwapEvent as ProtocolEvent>::NAME, "Swap");
    }

    /// An event of a different type routes to `None` rather than erroring, on
    /// a real type rather than a test double.
    #[test]
    fn a_real_event_ignores_a_foreign_discriminator() {
        let mut data = crate::parsing::anchor::ANCHOR_EVENT_TAG.to_vec();
        data.extend_from_slice(&[0xAA; 8]);
        data.extend_from_slice(&[0u8; 128]);
        assert!(SwapEvent::from_event_instruction(&data).unwrap().is_none());
    }
}
