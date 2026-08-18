//! PumpSwap `CollectCoinCreatorFeeEvent`.
//!
//! PumpSwap's name for the same fact pumpfun calls a creator fee: the launcher
//! of the token ("coin creator") withdrawing fees the AMM accrued for them.
//! Modelled separately because the field list differs — it names the two token
//! accounts the transfer ran between — but it normalises to the same
//! [`CreatorFee`](crate::chain::CreatorFee).
//!
//! Borsh over the IDL's field list. The sibling `BuyEvent`/`SellEvent` in this
//! module are still hand-counted byte offsets; this one is not, and new events
//! here should follow this file rather than those.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;

/// `sha256("event:CollectCoinCreatorFeeEvent")[..8]`, derived at compile time.
pub const COLLECT_COIN_CREATOR_FEE_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("CollectCoinCreatorFeeEvent");

/// A coin creator withdrawing accrued AMM fees.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    solana_protocols_macros::EventLayout,
)]
#[idl(program = "pump_amm", event = "CollectCoinCreatorFeeEvent")]
pub struct CollectCoinCreatorFeeEvent {
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `coin_creator` — declared by the program IDL.
    pub coin_creator: Pubkey,
    /// `coin_creator_fee` — declared by the program IDL.
    pub coin_creator_fee: u64,
    /// `coin_creator_vault_ata` — declared by the program IDL.
    pub coin_creator_vault_ata: Pubkey,
    /// `coin_creator_token_account` — declared by the program IDL.
    pub coin_creator_token_account: Pubkey,
}

impl ProtocolEvent for CollectCoinCreatorFeeEvent {
    const DISCRIMINATOR: [u8; 8] = COLLECT_COIN_CREATOR_FEE_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "CollectCoinCreatorFeeEvent";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_round_trips_and_refuses_trailing_bytes() {
        let ev = CollectCoinCreatorFeeEvent {
            timestamp: 1_700_000_000,
            coin_creator: Pubkey::new_from_array([1; 32]),
            coin_creator_fee: 42,
            coin_creator_vault_ata: Pubkey::new_from_array([2; 32]),
            coin_creator_token_account: Pubkey::new_from_array([3; 32]),
        };
        let mut bytes = borsh::to_vec(&ev).expect("serialize");
        assert_eq!(bytes.len(), 8 + 32 + 8 + 32 + 32);
        assert_eq!(
            CollectCoinCreatorFeeEvent::from_event_body(&bytes).expect("decode"),
            ev
        );
        bytes.push(0);
        assert!(CollectCoinCreatorFeeEvent::from_event_body(&bytes).is_err());
    }

    /// Same fact, different program, different discriminator — a shared
    /// constant would silently cross-match the two protocols' events.
    #[test]
    fn it_does_not_collide_with_pumpfuns_collect_event() {
        assert_ne!(
            CollectCoinCreatorFeeEvent::DISCRIMINATOR,
            crate::protocols::pumpfun::events::COLLECT_CREATOR_FEE_EVENT_DISCRIMINATOR
        );
    }
}
