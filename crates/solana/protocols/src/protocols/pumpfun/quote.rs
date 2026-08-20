//! The state bundle a Pumpfun bonding-curve quote needs.
//!
//! A curve account carries its own (virtual) reserves, so unlike PumpSwap it
//! can answer *price* alone. What it cannot answer is *fees*: pumpfun keys
//! those on the curve's **market cap**, through the `FeeConfig` singleton the
//! `pump_fees` program owns. Market cap moves continuously as a curve fills,
//! which makes the compiled-in `PROTOCOL_FEE_BPS` / `CREATOR_FEE_BPS` pair the
//! quote used before this bundle correct at exactly one point on the curve and
//! wrong everywhere else — a sharper version of PumpSwap's hardcoded ladder,
//! because here the error is guaranteed to move rather than merely to be stale.
//!
//! Both halves of the lookup already existed and had no callers:
//! [`bonding_curve_market_cap`] and [`calculate_fee_tier`], documented down to
//! Pumpfun's below-floor convention. This bundle wires them.

use solana_protocols_macros::QuoteState;

use super::fee_config::{
    bonding_curve_market_cap, calculate_fee_tier, PumpfunFeeConfig, PumpfunFees,
};
use super::state::BondingCurve;
use crate::events::SwapOutput;
use crate::traits::{SwapMath, SwapParams};
use crate::Result;

/// Everything required to price a Pumpfun bonding-curve swap.
///
/// Every field declares how it is sourced; `#[derive(QuoteState)]` turns that
/// one list into both `assemble` and `dependent_accounts`, so what the ingest
/// layer keeps live and what the quoter reads cannot drift apart.
///
/// There is deliberately no constructor taking parts: a replay harness
/// satisfies `assemble`'s bound with its own cache, so recorded swaps run the
/// path production runs.
#[derive(Debug, Clone, QuoteState)]
pub struct PumpfunQuote {
    /// The curve account — reserves included, unlike an AMM pool.
    #[dep(root)]
    curve: BondingCurve,
    /// `pump_fees` fee schedule. A singleton, so not a keyed dependency.
    #[dep(singleton)]
    fee_config: PumpfunFeeConfig,
    /// Rates for *this* curve, selected at assembly from the tier its market
    /// cap falls in — never a rate compiled into the binary.
    #[dep(computed = calculate_fee_tier(
        &fee_config.fee_tiers,
        bonding_curve_market_cap(
            curve.virtual_quote_reserves,
            curve.virtual_token_reserves,
            curve.token_total_supply,
        ),
    ))]
    fees: PumpfunFees,
}

impl PumpfunQuote {
    #[must_use]
    pub const fn curve(&self) -> &BondingCurve {
        &self.curve
    }

    /// Fee rates this quote prices with.
    #[must_use]
    pub const fn fees(&self) -> &PumpfunFees {
        &self.fees
    }

    /// The config the [`fees`](Self::fees) were selected from — kept as
    /// provenance, and so a verifier can replay alternate selection rules
    /// against the same assembled state.
    #[must_use]
    pub const fn fee_config(&self) -> &PumpfunFeeConfig {
        &self.fee_config
    }

    /// This curve's market cap in lamports — the quantity the fee tier keys on.
    #[must_use]
    pub fn market_cap_lamports(&self) -> u128 {
        bonding_curve_market_cap(
            self.curve.virtual_quote_reserves,
            self.curve.virtual_token_reserves,
            self.curve.token_total_supply,
        )
    }
}

impl SwapMath for PumpfunQuote {
    /// Price a swap off the assembled bundle.
    ///
    /// Same curve math as the bare-account path; the difference is provenance —
    /// fee rates are read from chain rather than compiled in.
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        self.curve.calculate_swap_with_fees(params, &self.fees)
    }

    fn spot_price(&self) -> f64 {
        self.curve.spot_price()
    }

    fn is_active(&self) -> bool {
        self.curve.is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::pumpfun::fee_config::PumpfunFeeTier;

    fn tier(threshold: u128, protocol: u64, creator: u64) -> PumpfunFeeTier {
        PumpfunFeeTier {
            market_cap_lamports_threshold: threshold,
            fees: PumpfunFees {
                lp_fee_bps: 0,
                protocol_fee_bps: protocol,
                creator_fee_bps: creator,
            },
        }
    }

    /// A curve's market cap is derivable from its own account, which is why
    /// pumpfun can select its tier without reading anything but the config.
    ///
    /// Asserted as a *relationship* rather than a typed constant — the first
    /// version of this test carried a hand-rounded figure and was wrong by 6.5
    /// SOL, which is precisely how a transcribed vector fails.
    #[test]
    fn market_cap_comes_from_the_curves_own_reserves() {
        let (sol, tokens, supply) = (
            30_000_000_000u64,
            1_073_000_000_000_000u64,
            1_000_000_000_000_000u64,
        );
        let cap = bonding_curve_market_cap(sol, tokens, supply);
        assert_eq!(
            cap,
            u128::from(sol) * u128::from(supply) / u128::from(tokens)
        );
        assert_eq!(cap, 27_958_993_476);

        // It scales with the SOL side: a curve that has taken more SOL is
        // worth more, which is what moves it between fee tiers.
        assert!(bonding_curve_market_cap(sol * 2, tokens, supply) > cap);
        // And a zero-token curve cannot be priced rather than dividing by zero.
        assert_eq!(bonding_curve_market_cap(sol, 0, supply), 0);
    }

    /// Tier selection tracks the curve as it fills — the property a compiled-in
    /// rate cannot have. A curve early on its bonding curve and the same curve
    /// near graduation land in different tiers.
    #[test]
    fn a_filling_curve_moves_between_tiers() {
        let tiers = vec![tier(0, 100, 100), tier(50_000_000_000, 10, 10)];
        let early = calculate_fee_tier(&tiers, 20_000_000_000);
        let late = calculate_fee_tier(&tiers, 80_000_000_000);
        assert_eq!(early.protocol_fee_bps, 100);
        assert_eq!(late.protocol_fee_bps, 10);
    }
}
