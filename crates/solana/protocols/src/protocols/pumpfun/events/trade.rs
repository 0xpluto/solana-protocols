//! Pump.fun trade event parsing.

use solana_program::pubkey::Pubkey;

pub use crate::protocols::Shareholder;

/// `TradeEvent`'s own discriminator — `sha256("event:TradeEvent")[..8]`,
/// derived at compile time.
///
/// This is the tag at bytes `[8..16]` of an `emit_cpi!` event instruction, and
/// the tag at bytes `[0..8]` of an `emit!` `Program data:` log. It is **not**
/// [`ANCHOR_EVENT_TAG`], which prefixes every Anchor event on every program.
///
/// Until 2026-08-11 this constant held `ANCHOR_EVENT_TAG`'s bytes under this
/// name, and the extractor matched on it while never checking `[8..16]` — so
/// any sufficiently long Anchor event from pumpfun could be accepted as a
/// trade, and the `emit!` log path (which carries *this* value, verified
/// against 25 mainnet transactions) could never match at all.
///
/// [`ANCHOR_EVENT_TAG`]: crate::parsing::anchor::ANCHOR_EVENT_TAG
pub const TRADE_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("TradeEvent");

/// Trade event emitted by the pump.fun program.
///
/// Contains all trade data plus computed convenience methods.
/// Decoded by `pumpfun::extract`, which is the single parser for this layout.
/// A `#[derive(LogParser)]` used to sit here generating a *second* one that
/// nothing called — and being field-derived, it stopped at the pre-fee 121-byte
/// layout, so the dead parser was also the wrong one.
#[derive(
    Debug,
    Clone,
    Default,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    solana_protocols_macros::EventLayout,
)]
#[idl(program = "pump", event = "TradeEvent")]
pub struct TradeEvent {
    /// `mint` — declared by the program IDL.
    pub mint: Pubkey,
    /// `sol_amount` — declared by the program IDL.
    pub sol_amount: u64,
    /// `token_amount` — declared by the program IDL.
    pub token_amount: u64,
    /// `is_buy` — declared by the program IDL.
    pub is_buy: bool,
    /// `user` — declared by the program IDL.
    pub user: Pubkey,
    /// `timestamp` — declared by the program IDL.
    pub timestamp: i64,
    /// `virtual_sol_reserves` — declared by the program IDL.
    pub virtual_sol_reserves: u64,
    /// `virtual_token_reserves` — declared by the program IDL.
    pub virtual_token_reserves: u64,
    /// `real_sol_reserves` — declared by the program IDL.
    pub real_sol_reserves: u64,
    /// `real_token_reserves` — declared by the program IDL.
    pub real_token_reserves: u64,
    /// `fee_recipient` — declared by the program IDL.
    pub fee_recipient: Pubkey,
    /// `fee_basis_points` — declared by the program IDL.
    pub fee_basis_points: u64,
    /// `fee` — declared by the program IDL.
    pub fee: u64,
    /// `creator` — declared by the program IDL.
    pub creator: Pubkey,
    /// `creator_fee_basis_points` — declared by the program IDL.
    pub creator_fee_basis_points: u64,
    /// `creator_fee` — declared by the program IDL.
    pub creator_fee: u64,
    /// `track_volume` — declared by the program IDL.
    pub track_volume: bool,
    /// `total_unclaimed_tokens` — declared by the program IDL.
    pub total_unclaimed_tokens: u64,
    /// `total_claimed_tokens` — declared by the program IDL.
    pub total_claimed_tokens: u64,
    /// `current_sol_volume` — declared by the program IDL.
    pub current_sol_volume: u64,
    /// `last_update_timestamp` — declared by the program IDL.
    pub last_update_timestamp: i64,
    /// `ix_name` — declared by the program IDL.
    pub ix_name: String,
    /// `mayhem_mode` — declared by the program IDL.
    pub mayhem_mode: bool,
    /// `cashback_fee_basis_points` — declared by the program IDL.
    pub cashback_fee_basis_points: u64,
    /// `cashback` — declared by the program IDL.
    pub cashback: u64,
    /// `buyback_fee_basis_points` — declared by the program IDL.
    pub buyback_fee_basis_points: u64,
    /// `buyback_fee` — declared by the program IDL.
    pub buyback_fee: u64,
    /// `shareholders` — declared by the program IDL.
    pub shareholders: Vec<Shareholder>,
    /// `quote_mint` — declared by the program IDL.
    pub quote_mint: Pubkey,
    /// `quote_amount` — declared by the program IDL.
    pub quote_amount: u64,
    /// `virtual_quote_reserves` — declared by the program IDL.
    pub virtual_quote_reserves: u64,
    /// `real_quote_reserves` — declared by the program IDL.
    pub real_quote_reserves: u64,
}

impl crate::parsing::event::ProtocolEvent for TradeEvent {
    const DISCRIMINATOR: [u8; 8] = TRADE_EVENT_DISCRIMINATOR;
    const NAME: &'static str = "TradeEvent";
}

impl TradeEvent {
    /// SOL amount in UI units (SOL, not lamports).
    #[must_use]
    pub fn sol_amount_ui(&self) -> f64 {
        self.sol_amount as f64 / 1e9
    }

    /// Token amount in UI units (tokens, not smallest unit).
    #[must_use]
    pub fn token_amount_ui(&self) -> f64 {
        self.token_amount as f64 / 1e6
    }

    /// Effective price of this trade (SOL per token, UI units).
    #[must_use]
    pub fn price(&self) -> f64 {
        let token_ui = self.token_amount_ui();
        if token_ui > 0.0 {
            self.sol_amount_ui() / token_ui
        } else {
            0.0
        }
    }

    /// Post-trade spot price (from reserves, SOL per token).
    #[must_use]
    pub fn post_trade_price(&self) -> f64 {
        let virtual_sol_ui = self.virtual_sol_reserves as f64 / 1e9;
        let virtual_token_ui = self.virtual_token_reserves as f64 / 1e6;
        if virtual_token_ui > 0.0 {
            virtual_sol_ui / virtual_token_ui
        } else {
            0.0
        }
    }

    /// Market cap estimate in SOL (based on post-trade reserves).
    #[must_use]
    pub fn market_cap_sol(&self) -> f64 {
        // Assuming 1B total supply at 6 decimals
        const TOTAL_SUPPLY: f64 = 1_000_000_000.0;
        self.post_trade_price() * TOTAL_SUPPLY
    }

    /// Bonding curve graduation progress (0.0 to 1.0).
    #[must_use]
    pub fn graduation_progress(&self) -> f64 {
        // Graduation typically happens around 85 SOL
        const GRADUATION_THRESHOLD: f64 = 85.0;
        let real_sol_ui = self.real_sol_reserves as f64 / 1e9;
        (real_sol_ui / GRADUATION_THRESHOLD).min(1.0)
    }

    /// Price impact of this trade in basis points.
    ///
    /// Compares effective price to post-trade spot price.
    #[must_use]
    pub fn price_impact_bps(&self) -> u16 {
        let effective = self.price();
        let spot = self.post_trade_price();
        if spot == 0.0 {
            return 0;
        }
        let impact = ((effective - spot) / spot).abs();
        (impact * 10000.0) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_event_discriminator() {
        // Pinned against the Anchor rule, not against itself.
        assert_eq!(
            TRADE_EVENT_DISCRIMINATOR,
            solana_protocols_macros::anchor_event_discriminator!("TradeEvent")
        );
    }

    #[test]
    fn trade_event_computed_fields() {
        let event = TradeEvent {
            mint: Pubkey::new_unique(),
            sol_amount: 1_000_000_000,     // 1 SOL
            token_amount: 100_000_000_000, // 100K tokens
            is_buy: true,
            user: Pubkey::new_unique(),
            timestamp: 1700000000,
            virtual_sol_reserves: 31_000_000_000,
            virtual_token_reserves: 900_000_000_000_000,
            real_sol_reserves: 1_000_000_000,
            real_token_reserves: 700_000_000_000_000,
            ..Default::default()
        };

        assert!(event.is_buy);
        assert!((event.sol_amount_ui() - 1.0).abs() < 0.0001);
        assert!((event.token_amount_ui() - 100_000.0).abs() < 0.1);

        // Price should be 1 SOL / 100K tokens = 0.00001
        assert!((event.price() - 0.00001).abs() < 0.0000001);

        // Post-trade price from reserves
        let expected_post_price = 31.0 / 900_000_000.0;
        assert!((event.post_trade_price() - expected_post_price).abs() < 0.0000000001);
    }

    #[test]
    fn graduation_progress() {
        let event = TradeEvent {
            mint: Pubkey::new_unique(),
            sol_amount: 0,
            token_amount: 0,
            is_buy: true,
            user: Pubkey::new_unique(),
            timestamp: 0,
            virtual_sol_reserves: 0,
            virtual_token_reserves: 0,
            real_sol_reserves: 42_500_000_000, // 42.5 SOL = 50%
            real_token_reserves: 0,
            ..Default::default()
        };

        assert!((event.graduation_progress() - 0.5).abs() < 0.01);
    }
}
