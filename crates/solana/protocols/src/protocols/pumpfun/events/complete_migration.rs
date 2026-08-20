//! Pump.fun `CompletePumpAmmMigrationEvent`.
//!
//! The graduation settling, from pumpfun's side. Unlike the pumpswap
//! `CreatePoolEvent`, this names the source `bonding_curve` and the
//! `pool_migration_fee` — the pumpfun half of a fact split across two programs.
//!
//! Borsh over the IDL's field list, pinned to real captured bodies. A synthetic
//! body serialized by the struct that decodes it proves only that the struct
//! agrees with itself.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;

/// `sha256("event:CompletePumpAmmMigrationEvent")[..8]`, derived at compile time.
pub const COMPLETE_PUMP_AMM_MIGRATION_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("CompletePumpAmmMigrationEvent");

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
#[idl(program = "pump", event = "CompletePumpAmmMigrationEvent")]
pub struct CompletePumpAmmMigrationEvent {
    /// `user` — declared by the program IDL.
    pub user: Pubkey,
    /// `mint` — declared by the program IDL.
    pub mint: Pubkey,
    /// `mint_amount` — declared by the program IDL.
    pub mint_amount: u64,
    /// `sol_amount` — declared by the program IDL.
    pub sol_amount: u64,
    /// `pool_migration_fee` — declared by the program IDL.
    pub pool_migration_fee: u64,
    /// `bonding_curve` — declared by the program IDL.
    pub bonding_curve: Pubkey,
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `pool` — declared by the program IDL.
    pub pool: Pubkey,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
}

impl ProtocolEvent for CompletePumpAmmMigrationEvent {
    const DISCRIMINATOR: [u8; 8] = COMPLETE_PUMP_AMM_MIGRATION_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "CompletePumpAmmMigrationEvent";
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
            CompletePumpAmmMigrationEvent::from_event_body(&b)
                .unwrap_or_else(|e| panic!("{n}-byte body must decode: {e}"));
            let mut longer = b;
            longer.push(0);
            assert!(
                CompletePumpAmmMigrationEvent::from_event_body(&longer).is_err(),
                "a byte past the last field means the program grew one"
            );
        }
        check(body(include_str!(
            "../../../../fixtures/pumpfun/event_bodies/CompletePumpAmmMigrationEvent_192.hex"
        )));
    }
}
