//! PumpSwap `CreatePoolEvent`.
//!
//! Modelled from the IDL's field list and pinned to a real captured body. The
//! IDL's declared size is exactly 326 bytes and every body the firehose produced
//! was 326 bytes, so there is no undeclared tail here — unlike `BuyEvent` and
//! `SellEvent`, whose real bodies run past what any IDL declares.
//!
//! Borsh over the field list, never hand-counted offsets.

use solana_program::pubkey::Pubkey;

use crate::parsing::event::ProtocolEvent;

/// `sha256("event:CreatePoolEvent")[..8]`, derived at compile time.
pub const CREATE_POOL_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("CreatePoolEvent");

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
#[idl(program = "pump_amm", event = "CreatePoolEvent")]
pub struct CreatePoolEvent {
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `index` — declared by the program IDL.
    pub index: u16,
    /// `creator` — declared by the program IDL.
    pub creator: Pubkey,
    /// `base_mint` — declared by the program IDL.
    pub base_mint: Pubkey,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
    /// `base_mint_decimals` — declared by the program IDL.
    pub base_mint_decimals: u8,
    /// `quote_mint_decimals` — declared by the program IDL.
    pub quote_mint_decimals: u8,
    /// `base_amount_in` — declared by the program IDL.
    pub base_amount_in: u64,
    /// `quote_amount_in` — declared by the program IDL.
    pub quote_amount_in: u64,
    /// `pool_base_amount` — declared by the program IDL.
    pub pool_base_amount: u64,
    /// `pool_quote_amount` — declared by the program IDL.
    pub pool_quote_amount: u64,
    /// `minimum_liquidity` — declared by the program IDL.
    pub minimum_liquidity: u64,
    /// `initial_liquidity` — declared by the program IDL.
    pub initial_liquidity: u64,
    /// `lp_token_amount_out` — declared by the program IDL.
    pub lp_token_amount_out: u64,
    /// `pool_bump` — declared by the program IDL.
    pub pool_bump: u8,
    /// `pool` — declared by the program IDL.
    pub pool: Pubkey,
    /// `lp_mint` — declared by the program IDL.
    pub lp_mint: Pubkey,
    /// `user_base_token_account` — declared by the program IDL.
    pub user_base_token_account: Pubkey,
    /// `user_quote_token_account` — declared by the program IDL.
    pub user_quote_token_account: Pubkey,
    /// `coin_creator` — declared by the program IDL.
    pub coin_creator: Pubkey,
    /// `is_mayhem_mode` — declared by the program IDL.
    pub is_mayhem_mode: bool,
}

impl ProtocolEvent for CreatePoolEvent {
    const DISCRIMINATOR: [u8; 8] = CREATE_POOL_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "CreatePoolEvent";
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
        let hex = include_str!("../../../../fixtures/pumpswap/event_bodies/CreatePoolEvent_326.hex");
        let body: Vec<u8> = (0..hex.trim().len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex.trim()[i..i + 2], 16).expect("hex"))
            .collect();
        assert_eq!(body.len(), 326, "the captured body is the size the IDL declares");
        let ev = CreatePoolEvent::from_event_body(&body).expect("real body decodes");

        let mut longer = body.clone();
        longer.push(0);
        assert!(
            CreatePoolEvent::from_event_body(&longer).is_err(),
            "a byte past the last field means the program grew one"
        );
        let _ = ev;
    }
}
