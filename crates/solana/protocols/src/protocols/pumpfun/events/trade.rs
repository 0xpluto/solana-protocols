//! Pump.fun trade event parsing.

use solana_program::pubkey::Pubkey;

use crate::parsing::state::Legacy;

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
#[derive(Debug, Clone)]
pub struct TradeEvent {
    /// Token mint address.
    pub mint: Pubkey,
    /// SOL amount in lamports.
    pub sol_amount: u64,
    /// Token amount in smallest units.
    pub token_amount: u64,
    /// Whether this is a buy (true) or sell (false).
    pub is_buy: bool,
    /// User who made the trade.
    pub user: Pubkey,
    /// Unix timestamp of the trade.
    pub timestamp: i64,
    /// Virtual SOL reserves after trade.
    pub virtual_sol_reserves: u64,
    /// Virtual token reserves after trade.
    pub virtual_token_reserves: u64,
    /// Real SOL reserves after trade.
    pub real_sol_reserves: u64,
    /// Real token reserves after trade.
    pub real_token_reserves: u64,
    /// Fees the chain charged for this trade, when the event carries them.
    ///
    /// [`Absent`](Legacy::Absent) means the event predates the fee fields
    /// (a 121-byte body), never "no fee was charged" — a trade with a real
    /// fee of zero is `Present` with zeroes in it.
    pub fees: Legacy<TradeFees>,
}

/// The fee split the chain published with a trade, in lamports.
///
/// Read straight off the event rather than recomputed. Pumpfun's rates are
/// tiered on market cap, so a local recomputation is a model of the chain's
/// answer where the event *is* the chain's answer; the sampled rates land on
/// [`PROTOCOL_FEE_BPS`] and [`CREATOR_FEE_BPS`] exactly, and the lamport
/// amounts round *up* — the same ceiling the PumpSwap fee grading found.
///
/// [`PROTOCOL_FEE_BPS`]: super::super::constants::PROTOCOL_FEE_BPS
/// [`CREATOR_FEE_BPS`]: super::super::constants::CREATOR_FEE_BPS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeFees {
    /// Which of the configured recipients this trade paid.
    pub fee_recipient: Pubkey,
    /// Protocol fee rate in basis points.
    pub fee_basis_points: u64,
    /// Protocol fee actually charged, in lamports.
    pub fee: u64,
    /// The coin creator (all-zero when the coin has none).
    pub creator: Pubkey,
    /// Creator fee rate in basis points.
    pub creator_fee_basis_points: u64,
    /// Creator fee actually charged, in lamports.
    pub creator_fee: u64,
}

impl TradeFees {
    /// Byte offset of the fee block within the event body.
    pub(crate) const OFFSET: usize = 121;
    /// Byte length of the fee block.
    pub(crate) const LEN: usize = 32 + 8 + 8 + 32 + 8 + 8;

    /// Total lamports the trade paid in fees.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.fee.saturating_add(self.creator_fee)
    }
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
            fees: Legacy::Absent,
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
            fees: Legacy::Absent,
        };

        assert!((event.graduation_progress() - 0.5).abs() < 0.01);
    }
}
