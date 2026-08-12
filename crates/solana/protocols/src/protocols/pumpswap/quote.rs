//! The state bundle a PumpSwap quote needs — assembled, not assumed.
//!
//! A quote's input is **several accounts**, not one. A PumpSwap pool account
//! holds no reserves (they live in two vault token accounts) and no fee rates
//! (they live in the `pump_fees` `FeeConfig` singleton). Modelling the quote's
//! input as the pool account alone had five consequences, all of them one bug:
//!
//! 1. `SwapMath` was implemented on [`PumpSwapPool`], so it could not reach any
//!    other account.
//! 2. Reserves therefore arrived through `set_reserves`/`with_reserves` — a
//!    mutator the caller had to *remember*. `from_account_data` zeroes them, so
//!    a pool read straight from the cache quotes on `0 / 0`: `is_active()` is
//!    false and `calculate_swap` errors. Exactly one production caller ever
//!    filled them.
//! 3. Fees could not come from the config, so they came from a 26-arm hardcoded
//!    ladder.
//! 4. The cached [`PumpSwapFeeConfig`] consequently had zero readers.
//! 5. That ladder keys on quote-reserve SOL while the on-chain tiers key on a
//!    market-cap threshold — see [`Fees::TIER_KEY_UNSETTLED`].
//!
//! [`PumpSwapQuote`] is the fix: it carries everything the math needs, and the
//! only way to build one from a cache runs the assembly. A partially-populated
//! quote is unrepresentable — `assemble` returns `None` rather than quoting on
//! a reserve it could not read.

use solana_protocols_macros::QuoteState;

use super::fee_config::PumpSwapFeeConfig;
use super::math::{calculate_swap_exact_in, calculate_swap_exact_out, FeeStructure};
use super::state::PumpSwapPool;
use crate::events::SwapOutput;
use crate::pumpfun::PumpfunFees;
use crate::tokens::TokenAccount;
use crate::traits::{SwapDirection, SwapMath, SwapParams};
use crate::Result;

/// Everything required to price a PumpSwap swap.
///
/// Every field declares how it is sourced, and `#[derive(QuoteState)]` turns
/// that one list into both `assemble` (read them out of a cache) and
/// `dependent_accounts` (tell the ingest layer to keep them live). Before the
/// derive those were two hand-written lists with nothing checking they agreed —
/// and an account present in one but not the other makes the pool silently
/// unquotable forever.
///
/// Fields are private: the invariant is that every one was *sourced*. There is
/// deliberately no `from_parts` — a replay harness satisfies `assemble`'s bound
/// with its own cache rather than bypassing assembly, so recorded swaps run
/// through exactly the path production uses.
#[derive(Debug, Clone, QuoteState)]
pub struct PumpSwapQuote {
    #[dep(root)]
    pool: PumpSwapPool,
    /// Base-side vault. Owned by the *token* program, not pumpswap, so no
    /// owner subscription covers it — it must be subscribed by pubkey.
    #[dep(key = pool.pool_base_token_account, expect = dynamic)]
    base: TokenAccount,
    /// Quote-side vault. Same delivery class as `base`.
    #[dep(key = pool.pool_quote_token_account, expect = dynamic)]
    quote: TokenAccount,
    /// On-chain fee schedule. A singleton, so not a keyed dependency.
    #[dep(singleton)]
    fee_config: PumpSwapFeeConfig,
    /// Selected once at assembly, from the config this quote was built with —
    /// never from a table compiled into the binary.
    #[dep(computed = Fees::select(&fee_config, pool.has_creator(), quote.balance))]
    fees: PumpfunFees,
}

impl PumpSwapQuote {
    #[must_use]
    pub const fn pool(&self) -> &PumpSwapPool {
        &self.pool
    }

    /// Base-side reserve — the vault balance, which the pool account does not
    /// carry.
    #[must_use]
    pub const fn base_reserves(&self) -> u64 {
        self.base.balance
    }

    /// Quote-side reserve — the vault balance.
    #[must_use]
    pub const fn quote_reserves(&self) -> u64 {
        self.quote.balance
    }

    #[must_use]
    pub const fn fees(&self) -> &PumpfunFees {
        &self.fees
    }

    /// The config the [`fees`](Self::fees) were selected from.
    ///
    /// Retained as provenance, and because which quantity selects the tier is
    /// unsettled ([`Fees::TIER_KEY_UNSETTLED`]): the verifier replays a real
    /// swap against alternate selection rules over this same assembled state,
    /// which it cannot do from the chosen rates alone.
    #[must_use]
    pub const fn fee_config(&self) -> &PumpSwapFeeConfig {
        &self.fee_config
    }

    /// Both sides funded — the pool can price a trade.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.base.balance > 0 && self.quote.balance > 0
    }
}

impl SwapMath for PumpSwapQuote {
    /// Price a swap off the assembled bundle.
    ///
    /// Same curve as the legacy pool impl; the difference is provenance —
    /// reserves are the vault balances this bundle was assembled from, and fee
    /// rates come from the on-chain config rather than a transcribed ladder.
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        if !self.is_active() {
            return Err(crate::Error::swap_error(
                "PumpSwap pool has no reserves on one or both sides",
            ));
        }
        let fees = FeeStructure {
            creator_bps: self.fees.creator_fee_bps,
            protocol_bps: self.fees.protocol_fee_bps,
            lp_bps: self.fees.lp_fee_bps,
        };
        // Buy: quote (SOL) in, base out. Sell: base in, quote out.
        let (reserve_in, reserve_out) = match params.direction {
            SwapDirection::Buy => (self.quote_reserves(), self.base_reserves()),
            SwapDirection::Sell => (self.base_reserves(), self.quote_reserves()),
        };
        let amount = params.amount.amount();
        if params.amount.is_exact_in() {
            let (_, output) = calculate_swap_exact_in(amount, reserve_in, reserve_out, &fees)?;
            Ok(output)
        } else {
            Ok(calculate_swap_exact_out(
                amount,
                reserve_in,
                reserve_out,
                &fees,
            ))
        }
    }

    fn spot_price(&self) -> f64 {
        if self.base_reserves() == 0 {
            return 0.0;
        }
        self.quote_reserves() as f64 / self.base_reserves() as f64
    }

    fn is_active(&self) -> bool {
        Self::is_active(self)
    }
}

/// Fee-rate selection from the on-chain config.
pub struct Fees;

impl Fees {
    /// **Settled — and this implementation is wrong.** See the const below.
    ///
    /// Measured 2026-08-10 by grading 400 executed swaps and reading the live
    /// config (`5PHirr8j…`, 25 tiers). The hardcoded ladder this replaced
    /// transcribed the table's *values* faithfully — thresholds 420/1470/2460,
    /// creator 30/95/90, the launch-phase (93,2) split — but keyed them on
    /// quote-reserve SOL, and the on-chain key is market cap.
    ///
    /// The error is ~2x: pools with 2,544–6,652 SOL of depth showed an implied
    /// 55–75bp, which is the `>=49,120` / `>=29,470` market-cap tier. Keyed on
    /// depth they would land in the 100–110bp tiers instead.
    ///
    /// Fixing it needs the base mint's **supply**, which this bundle does not
    /// read — market cap is supply x price, and the pool account carries
    /// neither. That is a new `#[dep]` field, not a one-line change, which is
    /// why the wrong key is left in place and labelled rather than swapped for
    /// a different guess.
    ///
    /// On-chain the tiers are `PumpfunFeeTier { market_cap_lamports_threshold,
    /// fees }`, sorted ascending, "applies when market cap >= threshold". The
    /// hardcoded ladder this replaces keyed on *quote-reserve SOL* instead,
    /// with thresholds (420, 1470, 2460 …) that look like SOL amounts rather
    /// than market caps. Market cap and quote reserves are different
    /// quantities, so at most one of those rules is right.
    ///
    /// Rather than guess — inventing a rule is the bug class this module
    /// exists to remove — selection is kept explicit and the swap verifier
    /// settles it empirically: replay real swaps and see which rule reproduces
    /// the observed `amount_out` exactly.
    pub const TIER_KEY_UNSETTLED: &'static str =
        "SETTLED 2026-08-10: the key is MARKET CAP, not quote reserves — this \
         implementation is wrong and selects a tier roughly 2x too expensive";

    /// Select the fee tier for a pool whose quote-side reserve is
    /// `quote_reserves` lamports.
    ///
    /// Carries the pre-existing keying (see [`TIER_KEY_UNSETTLED`]) so this
    /// change stays purely structural — what moved is *where fees come from*
    /// (the chain, not a transcribed table), not the selection rule, which the
    /// verifier will settle on evidence.
    ///
    /// [`TIER_KEY_UNSETTLED`]: Self::TIER_KEY_UNSETTLED
    #[must_use]
    pub fn select(
        config: &PumpSwapFeeConfig,
        has_coin_creator: bool,
        quote_reserves: u64,
    ) -> PumpfunFees {
        if !has_coin_creator {
            // No creator, no creator fee — the config's own flat rates apply
            // (lp 25 / protocol 5 / creator 0 = 30bp), which is why it carries
            // `flat_fees` *and* `fee_tiers` rather than tiers alone.
            //
            // Measured 2026-08-10 across 85 pools from the tape: 47 of 47 pools
            // with a `coin_creator` mis-price under a tier-only model, while 37
            // of 38 without one price exactly. One exception, uninvestigated.
            return config.0.flat_fees;
        }
        // One tier-lookup implementation, shared with pumpfun. Only the *key*
        // is in question here (see TIER_KEY_UNSETTLED) — the lookup rule, down
        // to Pumpfun's below-floor convention of returning the first tier, is
        // the same account layout from the same fee program, so re-deriving it
        // separately would only create a second thing to drift.
        crate::pumpfun::calculate_fee_tier(&config.0.fee_tiers, u128::from(quote_reserves))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pumpfun::{PumpfunFeeConfig, PumpfunFeeTier};
    use solana_program::pubkey::Pubkey;

    fn fees(lp: u64) -> PumpfunFees {
        PumpfunFees {
            lp_fee_bps: lp,
            protocol_fee_bps: 5,
            creator_fee_bps: 30,
        }
    }

    fn config(tiers: Vec<(u128, u64)>) -> PumpSwapFeeConfig {
        PumpSwapFeeConfig(PumpfunFeeConfig {
            bump: 0,
            admin: Pubkey::default(),
            flat_fees: fees(1),
            fee_tiers: tiers
                .into_iter()
                .map(|(threshold, lp)| PumpfunFeeTier {
                    market_cap_lamports_threshold: threshold,
                    fees: fees(lp),
                })
                .collect(),
        })
    }

    /// Tiers apply at-or-above their threshold, and the *highest* qualifying
    /// tier wins — not the first.
    #[test]
    fn selects_the_highest_qualifying_tier() {
        let c = config(vec![(0, 10), (100, 20), (200, 30)]);
        assert_eq!(Fees::select(&c, true, 0).lp_fee_bps, 10);
        assert_eq!(Fees::select(&c, true, 99).lp_fee_bps, 10);
        assert_eq!(
            Fees::select(&c, true, 100).lp_fee_bps,
            20,
            "boundary is inclusive"
        );
        assert_eq!(Fees::select(&c, true, 10_000).lp_fee_bps, 30);
    }

    /// A pool with no coin creator pays the config's flat rates, not a tier.
    ///
    /// This is what the hardcoded ladder got right and the first version of
    /// `select` dropped: the on-chain config carries `flat_fees` *and*
    /// `fee_tiers` because both cases exist. Measured across 85 tape pools —
    /// 47/47 with a creator mis-price under a tier-only model, 37/38 without
    /// one price exactly.
    #[test]
    fn a_pool_without_a_coin_creator_pays_flat_fees_not_a_tier() {
        let c = config(vec![(0, 10), (100, 20)]);
        // flat_fees in the fixture carry lp = 1; the tiers carry 10 and 20.
        assert_eq!(Fees::select(&c, false, 10_000).lp_fee_bps, 1);
        assert_eq!(Fees::select(&c, true, 10_000).lp_fee_bps, 20);
    }

    /// A config with no tiers yields zero fees rather than a compiled-in
    /// guess — the shared lookup's defensive case, which should never occur
    /// against a live FeeConfig.
    #[test]
    fn no_tiers_yields_zero_fees_not_a_guess() {
        assert_eq!(Fees::select(&config(vec![]), true, 5_000).lp_fee_bps, 0);
    }

    /// Below the lowest threshold, Pumpfun's convention is the *first* tier —
    /// not the flat fees, and not a zero that would read as "free".
    #[test]
    fn below_every_threshold_uses_the_first_tier() {
        let c = config(vec![(500, 20)]);
        assert_eq!(Fees::select(&c, true, 499).lp_fee_bps, 20);
    }
}
