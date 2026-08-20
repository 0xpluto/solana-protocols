//! Pump.fun `CompleteEvent`.
//!
//! The bonding curve filling up: the moment a coin stops trading on the curve
//! and becomes eligible to migrate. Carries no amounts — it is the signal, not
//! the settlement; `CompletePumpAmmMigrationEvent` carries what actually moved.
//!
//! Borsh over the IDL's field list, pinned to real captured bodies. A synthetic
//! body serialized by the struct that decodes it proves only that the struct
//! agrees with itself.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;

/// `sha256("event:CompleteEvent")[..8]`, derived at compile time.
pub const COMPLETE_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("CompleteEvent");

/// See the [module docs](self).
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
#[idl(program = "pump", event = "CompleteEvent")]
pub struct CompleteEvent {
    /// `user` — declared by the program IDL.
    pub user: Pubkey,
    /// `mint` — declared by the program IDL.
    pub mint: Pubkey,
    /// `bonding_curve` — declared by the program IDL.
    pub bonding_curve: Pubkey,
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
}

impl ProtocolEvent for CompleteEvent {
    const DISCRIMINATOR: [u8; 8] = COMPLETE_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "CompleteEvent";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(hex: &str) -> Vec<u8> {
        let h = hex.trim();
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// Every captured length decodes, and a trailing byte is refused.
    #[test]
    fn real_bodies_decode_and_refuse_trailing_bytes() {
        fn check(b: Vec<u8>) {
            let n = b.len();
            CompleteEvent::from_event_body(&b)
                .unwrap_or_else(|e| panic!("{n}-byte body must decode: {e}"));
            let mut longer = b;
            longer.push(0);
            assert!(
                CompleteEvent::from_event_body(&longer).is_err(),
                "a byte past the last field means the program grew one"
            );
        }
        check(body(include_str!(
            "../../../../fixtures/pumpfun/event_bodies/CompleteEvent_136.hex"
        )));
    }
}
