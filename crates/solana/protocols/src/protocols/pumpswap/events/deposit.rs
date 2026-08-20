//! PumpSwap `DepositEvent`.
//!
//! Modelled from the IDL's field list and pinned to a real captured body. The
//! IDL's declared size is exactly 248 bytes and every body the firehose produced
//! was 248 bytes, so there is no undeclared tail here — unlike `BuyEvent` and
//! `SellEvent`, whose real bodies run past what any IDL declares.
//!
//! Borsh over the field list, never hand-counted offsets.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;

/// `sha256("event:DepositEvent")[..8]`, derived at compile time.
pub const DEPOSIT_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("DepositEvent");

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
#[idl(program = "pump_amm", event = "DepositEvent")]
pub struct DepositEvent {
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `lp_token_amount_out` — declared by the program IDL.
    pub lp_token_amount_out: u64,
    /// `max_base_amount_in` — declared by the program IDL.
    pub max_base_amount_in: u64,
    /// `max_quote_amount_in` — declared by the program IDL.
    pub max_quote_amount_in: u64,
    /// `user_base_token_reserves` — declared by the program IDL.
    pub user_base_token_reserves: u64,
    /// `user_quote_token_reserves` — declared by the program IDL.
    pub user_quote_token_reserves: u64,
    /// `pool_base_token_reserves` — declared by the program IDL.
    pub pool_base_token_reserves: u64,
    /// `pool_quote_token_reserves` — declared by the program IDL.
    pub pool_quote_token_reserves: u64,
    /// `base_amount_in` — declared by the program IDL.
    pub base_amount_in: u64,
    /// `quote_amount_in` — declared by the program IDL.
    pub quote_amount_in: u64,
    /// `lp_mint_supply` — declared by the program IDL.
    pub lp_mint_supply: u64,
    /// `pool` — declared by the program IDL.
    pub pool: Pubkey,
    /// `user` — declared by the program IDL.
    pub user: Pubkey,
    /// `user_base_token_account` — declared by the program IDL.
    pub user_base_token_account: Pubkey,
    /// `user_quote_token_account` — declared by the program IDL.
    pub user_quote_token_account: Pubkey,
    /// `user_pool_token_account` — declared by the program IDL.
    pub user_pool_token_account: Pubkey,
}

impl ProtocolEvent for DepositEvent {
    const DISCRIMINATOR: [u8; 8] = DEPOSIT_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "DepositEvent";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real captured body decodes, and borsh refuses a trailing byte.
    ///
    /// A synthetic body serialized by the struct that decodes it proves only
    /// that the struct agrees with itself — which is how a pumpswap event
    /// conversion once shipped green while taking swaps to zero.
    #[test]
    fn the_real_body_decodes_and_refuses_a_trailing_byte() {
        let hex = include_str!("../../../../fixtures/pumpswap/event_bodies/DepositEvent_248.hex");
        let body: Vec<u8> = (0..hex.trim().len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex.trim()[i..i + 2], 16).expect("hex"))
            .collect();
        assert_eq!(
            body.len(),
            248,
            "the captured body is the size the IDL declares"
        );
        let ev = DepositEvent::from_event_body(&body).expect("real body decodes");

        let mut longer = body.clone();
        longer.push(0);
        assert!(
            DepositEvent::from_event_body(&longer).is_err(),
            "a byte past the last field means the program grew one"
        );
        let _ = ev;
    }
}
