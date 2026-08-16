//! Pump.fun creator-fee events.
//!
//! Pump charges a per-trade fee on the launcher's behalf and accrues it in a
//! `creator_vault` PDA. These are the two ways it comes back out, and they are
//! the only place the *amount* appears — the instructions themselves take no
//! arguments, and creator fees are paid in SOL, which the tape does not record
//! as balance deltas. Without these events the withdrawal is visible but the
//! size of it is not.
//!
//! Field lists are the IDL's, decoded by borsh rather than by hand-counted
//! offsets.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;
use crate::protocols::Shareholder;

/// `sha256("event:CollectCreatorFeeEvent")[..8]`, derived at compile time.
pub const COLLECT_CREATOR_FEE_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("CollectCreatorFeeEvent");

/// `sha256("event:DistributeCreatorFeesEvent")[..8]`, derived at compile time.
pub const DISTRIBUTE_CREATOR_FEES_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("DistributeCreatorFeesEvent");

/// A creator withdrawing accrued fees to their own account.
///
/// Note what is *absent*: no mint. The instruction drains the creator's vault,
/// which accrues across every token that creator launched, so this event
/// cannot be attributed to one token — only to a creator and a denomination.
#[derive(Debug, Clone, Default, PartialEq, Eq, borsh::BorshDeserialize, borsh::BorshSerialize)]
pub struct CollectCreatorFeeEvent {
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `creator` — declared by the program IDL.
    pub creator: Pubkey,
    /// `creator_fee` — declared by the program IDL.
    pub creator_fee: u64,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
}

impl ProtocolEvent for CollectCreatorFeeEvent {
    const DISCRIMINATOR: [u8; 8] = COLLECT_CREATOR_FEE_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "CollectCreatorFeeEvent";
}

/// Accrued fees split across a sharing config's shareholders.
///
/// Unlike a collect, this one *is* attributable: it names the mint and the
/// bonding curve whose trading earned the fees.
#[derive(Debug, Clone, Default, PartialEq, Eq, borsh::BorshDeserialize, borsh::BorshSerialize)]
pub struct DistributeCreatorFeesEvent {
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `mint` — declared by the program IDL.
    pub mint: Pubkey,
    /// `bonding_curve` — declared by the program IDL.
    pub bonding_curve: Pubkey,
    /// `sharing_config` — declared by the program IDL.
    pub sharing_config: Pubkey,
    /// `admin` — declared by the program IDL.
    pub admin: Pubkey,
    /// `shareholders` — declared by the program IDL.
    pub shareholders: Vec<Shareholder>,
    /// `distributed` — declared by the program IDL.
    pub distributed: u64,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
}

impl ProtocolEvent for DistributeCreatorFeesEvent {
    const DISCRIMINATOR: [u8; 8] = DISTRIBUTE_CREATOR_FEES_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "DistributeCreatorFeesEvent";
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    /// Borsh is length-prefixed for the shareholder vector, so an empty split
    /// and a populated one are both representable and distinguishable. A
    /// hand-offset reader could not have told them apart.
    #[test]
    fn a_distribute_event_round_trips_with_and_without_shareholders() {
        for holders in [
            vec![],
            vec![
                Shareholder {
                    address: Pubkey::new_from_array([7; 32]),
                    share_bps: 6_000,
                },
                Shareholder {
                    address: Pubkey::new_from_array([8; 32]),
                    share_bps: 4_000,
                },
            ],
        ] {
            let ev = DistributeCreatorFeesEvent {
                timestamp: 1,
                distributed: 123,
                shareholders: holders.clone(),
                ..Default::default()
            };
            let bytes = borsh::to_vec(&ev).expect("serialize");
            assert_eq!(
                DistributeCreatorFeesEvent::from_event_body(&bytes).expect("decode"),
                ev
            );
        }
    }

    /// Strict: trailing bytes are a layout disagreement, not slack.
    #[test]
    fn a_body_with_trailing_bytes_is_refused() {
        let ev = CollectCreatorFeeEvent {
            timestamp: 1,
            creator: Pubkey::new_from_array([3; 32]),
            creator_fee: 500,
            quote_mint: Pubkey::new_from_array([4; 32]),
        };
        let mut bytes = borsh::to_vec(&ev).expect("serialize");
        assert_eq!(
            CollectCreatorFeeEvent::from_event_body(&bytes).expect("decode"),
            ev
        );
        bytes.push(0);
        assert!(CollectCreatorFeeEvent::from_event_body(&bytes).is_err());
    }

    /// The two events must not share a discriminator, or dispatch between them
    /// is a coin flip.
    #[test]
    fn the_two_events_are_distinguishable() {
        assert_ne!(
            CollectCreatorFeeEvent::DISCRIMINATOR,
            DistributeCreatorFeesEvent::DISCRIMINATOR
        );
    }

    /// `BorshSerialize` is only in scope for the tests above; this keeps the
    /// import honest if they change.
    #[test]
    fn serialization_is_available_for_fixtures() {
        let mut buf = Vec::new();
        CollectCreatorFeeEvent::default()
            .serialize(&mut buf)
            .expect("serialize");
        assert_eq!(buf.len(), 8 + 32 + 8 + 32);
    }
}
