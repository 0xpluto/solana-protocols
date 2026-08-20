//! Pump.fun `CreateEvent`.
//!
//! A coin launching. The only event here with variable-width fields: three
//! borsh strings, which is why 47 distinct body lengths were captured in four
//! minutes and why length alone identifies nothing.
//!
//! Borsh over the IDL's field list, pinned to real captured bodies. A synthetic
//! body serialized by the struct that decodes it proves only that the struct
//! agrees with itself.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;

/// `sha256("event:CreateEvent")[..8]`, derived at compile time.
pub const CREATE_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("CreateEvent");

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
#[idl(program = "pump", event = "CreateEvent")]
pub struct CreateEvent {
    /// `name` — declared by the program IDL.
    pub name: String,
    /// `symbol` — declared by the program IDL.
    pub symbol: String,
    /// `uri` — declared by the program IDL.
    pub uri: String,
    /// `mint` — declared by the program IDL.
    pub mint: Pubkey,
    /// `bonding_curve` — declared by the program IDL.
    pub bonding_curve: Pubkey,
    /// `user` — declared by the program IDL.
    pub user: Pubkey,
    /// `creator` — declared by the program IDL.
    pub creator: Pubkey,
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `virtual_token_reserves` — declared by the program IDL.
    pub virtual_token_reserves: u64,
    /// `virtual_sol_reserves` — declared by the program IDL.
    pub virtual_sol_reserves: u64,
    /// `real_token_reserves` — declared by the program IDL.
    pub real_token_reserves: u64,
    /// `token_total_supply` — declared by the program IDL.
    pub token_total_supply: u64,
    /// `token_program` — declared by the program IDL.
    pub token_program: Pubkey,
    /// `is_mayhem_mode` — declared by the program IDL.
    pub is_mayhem_mode: bool,
    /// `is_cashback_enabled` — declared by the program IDL.
    pub is_cashback_enabled: bool,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
    /// `virtual_quote_reserves` — declared by the program IDL.
    pub virtual_quote_reserves: u64,
}

impl ProtocolEvent for CreateEvent {
    const DISCRIMINATOR: [u8; 8] = CREATE_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "CreateEvent";
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
            CreateEvent::from_event_body(&b)
                .unwrap_or_else(|e| panic!("{n}-byte body must decode: {e}"));
            let mut longer = b;
            longer.push(0);
            assert!(
                CreateEvent::from_event_body(&longer).is_err(),
                "a byte past the last field means the program grew one"
            );
        }
        check(body(include_str!(
            "../../../../fixtures/pumpfun/event_bodies/CreateEvent_308.hex"
        )));
        check(body(include_str!(
            "../../../../fixtures/pumpfun/event_bodies/CreateEvent_422.hex"
        )));
    }
}
