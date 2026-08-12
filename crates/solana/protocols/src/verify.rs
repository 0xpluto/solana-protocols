//! Grading our quote math against what the chain actually executed.
//!
//! A green test suite proves our math agrees with itself. This grades it
//! against reality: take a swap that landed, price it with the same code a
//! caller would use, and compare.
//!
//! # Exact, not approximate
//!
//! Constant-product quoting is integer arithmetic with floor division, so a
//! correct implementation reproduces `amount_out` **bit for bit**. There is no
//! tolerance band, which makes every mismatch a real defect rather than noise —
//! and makes the magnitude diagnostic rather than cosmetic. Hence
//! [`Outcome::Exact`] as the gate and [`Divergence`] as the description of how
//! a failure failed, never as a threshold that lets one pass.
//!
//! A prior art warning worth keeping: a verifier whose "exact" bucket was
//! `< 0.1%` would pass a quote off by 5bp — larger than the entire fee edge on
//! most of these pools.
//!
//! # Curve and fee model are graded separately
//!
//! A quote is a curve walk plus a fee deduction, and either can be wrong
//! independently. [`Grade::full`] uses the rates the quote state selected, so a
//! failure could be either. [`Grade::curve`] takes a quote state built with
//! **zero fees** and compares against the swap's pre-fee amount, reconstructed
//! from the fee the chain reported — so it fails only if the curve is wrong.
//! Running both tells you which half broke, a distinction a single pass/fail
//! conflates and the reason the PumpSwap tier-key question
//! (`Fees::TIER_KEY_UNSETTLED`) is answerable at all.
//!
//! Reconstructing the pre-fee amount needs to know which side the fee came off,
//! which is a protocol fact, not something the grader can infer: PumpSwap
//! deducts from the output, pumpfun from the input. Hence
//! [`ObservedSwap::fee_side`] — derivable from a swap's `fee_mint`.
//!
//! # What this module is not
//!
//! It performs no I/O and reads no tape. It takes an assembled quote state and
//! an observed swap and returns a verdict; where those come from — a live
//! cache, a replayed fixture — is the caller's problem. That keeps the grading
//! logic identical across every source, which is the entire point of grading.

use crate::traits::{SwapAmount, SwapDirection, SwapMath, SwapParams};

/// A swap that actually executed, as observed on chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedSwap {
    pub direction: SwapDirection,
    /// Gross amount that entered the swap.
    pub amount_in: u64,
    /// Amount delivered to the trader.
    pub amount_out: u64,
    /// Total fee the chain charged, when the protocol's event reports it.
    ///
    /// `None` is *undecoded*, not zero — pumpfun's extractor does not yet
    /// decode fees, and treating that as a zero fee would make every curve
    /// check fail for a reason that has nothing to do with the curve.
    pub fee_amount: Option<u64>,
    /// Which leg the fee was taken from.
    ///
    /// A protocol fact the grader cannot infer, and getting it wrong silently
    /// inverts the pre-fee reconstruction. Derivable from a swap's `fee_mint`:
    /// equal to `token_in` means [`FeeSide::Input`].
    pub fee_side: FeeSide,
}

/// Which leg of a swap a protocol takes its fee from.
///
/// PumpSwap deducts from the output (`amount_out` is already net); pumpfun
/// deducts from the input (the curve sees `amount_in - fee`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeSide {
    /// Fee comes off the input before the curve sees it.
    Input,
    /// Fee comes off the output after the curve produces it.
    Output,
}

/// Which property a verdict is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Given the input, does our math reproduce the output the chain produced?
    QuoteExactIn,
    /// Given the output, does our math reproduce the input the chain required?
    ///
    /// A separate code path in every AMM — different rounding direction, fee
    /// applied to a derived quantity — so passing `QuoteExactIn` says nothing
    /// about it.
    QuoteExactOut,
}

/// How far a prediction missed, bucketed by what that magnitude implicates.
///
/// The buckets name causes rather than sizes because that is what a reader
/// needs: an off-by-one is a rounding direction, a fee-sized miss is the wrong
/// tier, and anything larger means the curve or the state was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divergence {
    /// Off by a couple of raw units — a floor/ceil direction, not a model bug.
    Rounding,
    /// Off by up to a percent — the size a wrong fee rate produces.
    FeeRate,
    /// Off by more than a percent — wrong curve, or the state was not the
    /// state the swap executed against.
    Structural,
}

impl Divergence {
    /// Classify by relative error against what the chain produced.
    fn classify(predicted: u64, observed: u64) -> Self {
        let diff = predicted.abs_diff(observed);
        if diff <= 2 {
            return Self::Rounding;
        }
        // Relative to the observed value; a zero observation with a non-trivial
        // diff is structural by construction.
        match observed {
            0 => Self::Structural,
            _ if u128::from(diff) * 100 <= u128::from(observed) => Self::FeeRate,
            _ => Self::Structural,
        }
    }
}

/// The result of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Prediction matched the chain exactly. The only passing outcome.
    Exact,
    /// Prediction differed.
    Off {
        predicted: u64,
        observed: u64,
        divergence: Divergence,
    },
    /// Our math refused to quote at all.
    ///
    /// Counted separately from a wrong answer: a refusal is usually a missing
    /// input rather than bad math, and folding it into the mismatch rate would
    /// overstate how wrong the math is.
    Refused(String),
    /// The check could not run — the observation lacked something it needs.
    ///
    /// Never silently dropped: a skipped check that vanishes turns an accuracy
    /// rate into a number with an unknown denominator.
    Skipped(&'static str),
}

/// One graded check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub check: Check,
    pub outcome: Outcome,
}

impl Verdict {
    /// Did this check pass?
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.outcome, Outcome::Exact)
    }
}

/// Grades a quote implementation against observed swaps.
///
/// Generic over [`SwapMath`] deliberately: the grading logic is identical for
/// every protocol, so it is a function, not a macro. A macro here would produce
/// one copy of this per protocol and one place per protocol for the same bug.
pub struct Grade;

impl Grade {
    /// Grade the curve alone, with fees taken out of the comparison entirely.
    ///
    /// `zero_fee_quote` must be a quote state assembled with **zero** fee
    /// rates — otherwise its own deduction lands on top of the reconstruction
    /// below and the verdict is meaningless. It is a separate argument rather
    /// than something this function derives because only the caller can build
    /// one: fee rates come from a config account, and zeroing them means
    /// assembling from a cache that reports zeros.
    ///
    /// Isolates the two halves: if this passes and [`full`](Self::full) fails,
    /// the curve is right and fee selection is wrong.
    ///
    /// [`Outcome::Skipped`] when the observation carries no decoded fee —
    /// there is then nothing to reconstruct with.
    #[must_use]
    pub fn curve<Q: SwapMath>(zero_fee_quote: &Q, observed: &ObservedSwap) -> Verdict {
        let Some(fee) = observed.fee_amount else {
            return Verdict {
                check: Check::QuoteExactIn,
                outcome: Outcome::Skipped("observation carries no decoded fee"),
            };
        };
        // Undo the protocol's deduction to recover what the curve itself
        // produced, then ask a fee-free quote to reproduce exactly that.
        let (curve_in, curve_out) = match observed.fee_side {
            FeeSide::Input => match observed.amount_in.checked_sub(fee) {
                Some(net) => (net, observed.amount_out),
                None => {
                    return Verdict {
                        check: Check::QuoteExactIn,
                        outcome: Outcome::Skipped("reported fee exceeds the input amount"),
                    }
                }
            },
            FeeSide::Output => match observed.amount_out.checked_add(fee) {
                Some(gross) => (observed.amount_in, gross),
                None => {
                    return Verdict {
                        check: Check::QuoteExactIn,
                        outcome: Outcome::Skipped("output plus fee overflows"),
                    }
                }
            },
        };
        Self::run(
            zero_fee_quote,
            Check::QuoteExactIn,
            curve_in,
            observed.direction,
            |o| o.amount_out,
        )
        .compare(curve_out)
    }

    /// Grade the whole quote — curve *and* the fee rates the state selected.
    #[must_use]
    pub fn full<Q: SwapMath>(quote: &Q, observed: &ObservedSwap) -> Verdict {
        Self::run(
            quote,
            Check::QuoteExactIn,
            observed.amount_in,
            observed.direction,
            |o| o.amount_out,
        )
        .compare(observed.amount_out)
    }

    /// Grade the inverse direction: given the output, does our math ask for the
    /// input the chain required?
    #[must_use]
    pub fn inverse<Q: SwapMath>(quote: &Q, observed: &ObservedSwap) -> Verdict {
        Self::run(
            quote,
            Check::QuoteExactOut,
            observed.amount_out,
            observed.direction,
            |o| o.amount_in,
        )
        .compare(observed.amount_in)
    }

    fn run<Q: SwapMath>(
        quote: &Q,
        check: Check,
        amount: u64,
        direction: SwapDirection,
        pick: fn(&crate::events::SwapOutput) -> u64,
    ) -> Pending {
        let params = SwapParams {
            direction,
            amount: match check {
                Check::QuoteExactIn => SwapAmount::ExactIn(amount),
                Check::QuoteExactOut => SwapAmount::ExactOut(amount),
            },
        };
        match quote.calculate_swap(&params) {
            Ok(out) => Pending {
                check,
                predicted: Some(pick(&out)),
                refusal: None,
            },
            Err(e) => Pending {
                check,
                predicted: None,
                refusal: Some(e.to_string()),
            },
        }
    }
}

/// A prediction awaiting its comparison.
struct Pending {
    check: Check,
    predicted: Option<u64>,
    refusal: Option<String>,
}

impl Pending {
    fn compare(self, observed: u64) -> Verdict {
        let outcome = match (self.predicted, self.refusal) {
            (Some(predicted), _) if predicted == observed => Outcome::Exact,
            (Some(predicted), _) => Outcome::Off {
                predicted,
                observed,
                divergence: Divergence::classify(predicted, observed),
            },
            (None, Some(reason)) => Outcome::Refused(reason),
            (None, None) => Outcome::Skipped("no prediction and no refusal"),
        };
        Verdict {
            check: self.check,
            outcome,
        }
    }
}

/// Running totals over many verdicts.
///
/// Every disposition is counted, including the ones that did not run. An
/// accuracy figure without its skips is a number with an unknown denominator —
/// the failure mode of the prior-art verifier, which discarded any prediction
/// more than 10x off as a timing artifact and so threw away exactly the cases
/// where the math was most wrong.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally {
    pub exact: u64,
    pub rounding: u64,
    pub fee_rate: u64,
    pub structural: u64,
    pub refused: u64,
    pub skipped: u64,
}

impl Tally {
    /// Fold one verdict in.
    pub fn record(&mut self, verdict: &Verdict) {
        match &verdict.outcome {
            Outcome::Exact => self.exact += 1,
            Outcome::Off { divergence, .. } => match divergence {
                Divergence::Rounding => self.rounding += 1,
                Divergence::FeeRate => self.fee_rate += 1,
                Divergence::Structural => self.structural += 1,
            },
            Outcome::Refused(_) => self.refused += 1,
            Outcome::Skipped(_) => self.skipped += 1,
        }
    }

    /// Checks that actually produced a comparison.
    #[must_use]
    pub const fn graded(&self) -> u64 {
        self.exact + self.rounding + self.fee_rate + self.structural
    }

    /// Exact-match rate over graded checks, or `None` when nothing graded.
    ///
    /// `None` rather than 0.0 or 1.0: "nothing ran" is not an accuracy.
    #[must_use]
    pub fn exact_rate(&self) -> Option<f64> {
        (self.graded() > 0).then(|| self.exact as f64 / self.graded() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SwapOutput;
    use crate::Error;

    /// A quote that answers whatever it was built with — lets the grader be
    /// tested independently of any protocol's math.
    struct Canned(std::result::Result<(u64, u64), ()>);

    impl SwapMath for Canned {
        fn calculate_swap(&self, params: &SwapParams) -> crate::Result<SwapOutput> {
            match self.0 {
                Ok((amount_in, amount_out)) => {
                    let mut out = SwapOutput::new(amount_in, amount_out, 0, 0, 0);
                    // exact-out callers read amount_in; exact-in read amount_out.
                    if matches!(params.amount, SwapAmount::ExactOut(_)) {
                        out.amount_in = amount_in;
                    }
                    Ok(out)
                }
                Err(()) => Err(Error::ZeroAmount),
            }
        }
        fn spot_price(&self) -> f64 {
            0.0
        }
        fn is_active(&self) -> bool {
            true
        }
    }

    fn observed(amount_in: u64, amount_out: u64, fee: Option<u64>) -> ObservedSwap {
        ObservedSwap {
            direction: SwapDirection::Buy,
            amount_in,
            amount_out,
            fee_amount: fee,
            fee_side: FeeSide::Input,
        }
    }

    #[test]
    fn an_exact_match_is_the_only_pass() {
        let v = Grade::full(&Canned(Ok((1_000, 500))), &observed(1_000, 500, None));
        assert!(v.passed());

        // One raw unit off is still a failure, merely a well-classified one.
        let v = Grade::full(&Canned(Ok((1_000, 501))), &observed(1_000, 500, None));
        assert!(!v.passed());
        assert!(matches!(
            v.outcome,
            Outcome::Off {
                divergence: Divergence::Rounding,
                ..
            }
        ));
    }

    /// The magnitude has to point at a cause, or the bucket is decoration.
    #[test]
    fn divergence_buckets_separate_rounding_from_fees_from_structure() {
        assert_eq!(Divergence::classify(1_000, 1_002), Divergence::Rounding);
        // 0.5% off — the size a wrong fee tier moves an output.
        assert_eq!(Divergence::classify(100_500, 100_000), Divergence::FeeRate);
        // 10% off — not a fee, a different curve or a different state.
        assert_eq!(
            Divergence::classify(110_000, 100_000),
            Divergence::Structural
        );
        // A zero observation cannot be a rounding story.
        assert_eq!(Divergence::classify(50_000, 0), Divergence::Structural);
    }

    /// A refusal is not a wrong answer, and must not inflate the mismatch rate.
    #[test]
    fn a_refusal_is_counted_apart_from_a_mismatch() {
        let v = Grade::full(&Canned(Err(())), &observed(1_000, 500, None));
        assert!(matches!(v.outcome, Outcome::Refused(_)));

        let mut tally = Tally::default();
        tally.record(&v);
        assert_eq!(tally.refused, 1);
        assert_eq!(tally.graded(), 0, "a refusal grades nothing");
        assert_eq!(tally.exact_rate(), None, "nothing ran, so there is no rate");
    }

    /// Undecoded fees skip the curve check rather than being read as zero.
    ///
    /// pumpfun reports `fee_amount = 0` on every tape row because its extractor
    /// does not decode fees. Treating that as "no fee was charged" would make
    /// the curve check fail on every pumpfun swap for a reason that has nothing
    /// to do with the curve.
    #[test]
    fn a_missing_fee_skips_the_curve_check_rather_than_assuming_zero() {
        let v = Grade::curve(&Canned(Ok((1_000, 500))), &observed(1_000, 500, None));
        assert!(matches!(v.outcome, Outcome::Skipped(_)));

        let mut tally = Tally::default();
        tally.record(&v);
        assert_eq!(tally.skipped, 1);
        assert_eq!(tally.graded(), 0);
    }

    /// Input-side fees: the curve saw the input net of the fee.
    #[test]
    fn an_input_side_fee_is_removed_from_the_input() {
        // Chain took 10 of the 1,000 in, so the curve saw 990 and produced 500.
        let v = Grade::curve(&Canned(Ok((990, 500))), &observed(1_000, 500, Some(10)));
        assert!(v.passed());
    }

    /// Output-side fees: the curve produced *more* than the trader received.
    ///
    /// Getting this side backwards is the defect this test exists for — it
    /// would net the fee off the wrong leg and mis-grade every PumpSwap swap,
    /// which deducts from the output while pumpfun deducts from the input.
    #[test]
    fn an_output_side_fee_is_added_back_to_the_output() {
        let obs = ObservedSwap {
            direction: SwapDirection::Buy,
            amount_in: 1_000,
            amount_out: 490,
            fee_amount: Some(10),
            fee_side: FeeSide::Output,
        };
        // Curve produced 500 gross; trader got 490 after a 10 fee.
        assert!(Grade::curve(&Canned(Ok((1_000, 500))), &obs).passed());
        // Netting off the wrong leg would have expected 490 and passed a
        // curve that produced it — this pins that it does not.
        assert!(!Grade::curve(&Canned(Ok((1_000, 490))), &obs).passed());
    }

    /// A fee larger than the input is malformed data, not a curve verdict.
    #[test]
    fn a_fee_exceeding_the_input_skips_rather_than_underflows() {
        let v = Grade::curve(&Canned(Ok((0, 0))), &observed(100, 50, Some(500)));
        assert!(matches!(v.outcome, Outcome::Skipped(_)));
    }

    /// Exact-out is a distinct code path, so it gets a distinct verdict.
    #[test]
    fn the_inverse_direction_is_graded_separately() {
        let v = Grade::inverse(&Canned(Ok((1_000, 500))), &observed(1_000, 500, None));
        assert_eq!(v.check, Check::QuoteExactOut);
        assert!(v.passed());
    }

    #[test]
    fn a_tally_keeps_every_disposition() {
        let mut tally = Tally::default();
        for v in [
            Grade::full(&Canned(Ok((1_000, 500))), &observed(1_000, 500, None)),
            Grade::full(&Canned(Ok((1_000, 501))), &observed(1_000, 500, None)),
            Grade::full(&Canned(Ok((1_000, 900))), &observed(1_000, 500, None)),
            Grade::full(&Canned(Err(())), &observed(1_000, 500, None)),
        ] {
            tally.record(&v);
        }
        assert_eq!(tally.exact, 1);
        assert_eq!(tally.rounding, 1);
        assert_eq!(tally.structural, 1);
        assert_eq!(tally.refused, 1);
        assert_eq!(tally.graded(), 3, "the refusal is excluded from the base");
        assert!((tally.exact_rate().unwrap() - 1.0 / 3.0).abs() < 1e-9);
    }
}

// =============================================================================
// Level 2 — state transition, and the pair as the unit of verification
// =============================================================================

/// A pool's reserves, normalised to (SOL side, token side).
///
/// [`CurveState::Reserves`] is *direction-relative* — `in_side`/`out_side`
/// depend on which way that particular swap went. Chaining two swaps requires
/// a fixed orientation, or a buy followed by a sell reads as the reserves
/// having swapped places.
///
/// [`CurveState::Reserves`]: crate::chain::CurveState
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolReserves {
    /// Reserve on the SOL/quote side.
    pub sol: u64,
    /// Reserve on the token/base side.
    pub tok: u64,
}

impl PoolReserves {
    /// Orient a direction-relative pair given whether SOL was the input.
    #[must_use]
    pub const fn from_sides(in_side: u64, out_side: u64, sol_is_input: bool) -> Self {
        if sol_is_input {
            Self {
                sol: in_side,
                tok: out_side,
            }
        } else {
            Self {
                sol: out_side,
                tok: in_side,
            }
        }
    }

    /// Apply an executed trade under the **fee-stays-in-pool** model.
    ///
    /// The model is deliberately the naive one: the executed amounts are
    /// net of fees, so this predicts what the reserves become if the fee is
    /// left behind. Where the fee actually goes is the quantity we are trying
    /// to *measure*, and the residual against the next swap's observed state
    /// is the measurement — a transition wrong by exactly the fee says the fee
    /// left the pool. Encoding a guess here would destroy the experiment.
    #[must_use]
    pub const fn with_trade_applied(
        self,
        amount_in: u64,
        amount_out: u64,
        sol_is_input: bool,
    ) -> Self {
        if sol_is_input {
            Self {
                sol: self.sol.saturating_add(amount_in),
                tok: self.tok.saturating_sub(amount_out),
            }
        } else {
            Self {
                sol: self.sol.saturating_sub(amount_out),
                tok: self.tok.saturating_add(amount_in),
            }
        }
    }
}

/// What happened to one consecutive-swap pair.
///
/// Five outcomes rather than pass/fail, because "our model is wrong" and
/// "something else moved the pool" and "we could not run at all" call for
/// opposite responses, and an accuracy figure that hides the last two has an
/// unknown denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Predicted state equals the next swap's observed state.
    Pass,
    /// Both states are consistent with *a* swap, but not with ours.
    Mismatch,
    /// The pool moved in a way no swap of ours explains — a reserve that our
    /// trade must have increased came back smaller (or vice versa). Liquidity
    /// changed, funds were swept, or an unrecorded swap intervened.
    Discontinuous,
    /// Could not be checked: missing state, a non-SOL pair, or a quote that
    /// refused.
    Unverifiable,
}

/// Level 2: does our state model land on the next swap's observed state?
///
/// Returns the disposition and the signed residuals (`predicted − observed`)
/// per side. The residuals are the diagnostic: a residual equal to the fee on
/// exactly one side names fee routing, and a proportional residual on both
/// sides names a liquidity event rather than a math error.
#[must_use]
pub fn verify_transition(
    pre: PoolReserves,
    amount_in: u64,
    amount_out: u64,
    sol_is_input: bool,
    observed_next: PoolReserves,
) -> (Disposition, i128, i128) {
    let predicted = pre.with_trade_applied(amount_in, amount_out, sol_is_input);
    let d_sol = i128::from(predicted.sol) - i128::from(observed_next.sol);
    let d_tok = i128::from(predicted.tok) - i128::from(observed_next.tok);

    if predicted == observed_next {
        return (Disposition::Pass, 0, 0);
    }

    // Direction test: our trade forces a sign on each side's movement. If the
    // observed movement contradicts it, no version of our arithmetic produces
    // this state — something else touched the pool.
    let obs_sol = i128::from(observed_next.sol) - i128::from(pre.sol);
    let obs_tok = i128::from(observed_next.tok) - i128::from(pre.tok);
    let (want_sol_up, want_tok_up) = (sol_is_input, !sol_is_input);
    let sol_contradicts = (obs_sol > 0) != want_sol_up && obs_sol != 0;
    let tok_contradicts = (obs_tok > 0) != want_tok_up && obs_tok != 0;
    if sol_contradicts || tok_contradicts {
        return (Disposition::Discontinuous, d_sol, d_tok);
    }
    (Disposition::Mismatch, d_sol, d_tok)
}

/// Counts per [`Disposition`], so every pair reaches a reported outcome.
#[derive(Debug, Default, Clone)]
pub struct PairTally {
    pub pass: u64,
    pub mismatch: u64,
    pub discontinuous: u64,
    pub unverifiable: u64,
}

impl PairTally {
    /// Record one pair's disposition.
    pub fn record(&mut self, d: Disposition) {
        match d {
            Disposition::Pass => self.pass += 1,
            Disposition::Mismatch => self.mismatch += 1,
            Disposition::Discontinuous => self.discontinuous += 1,
            Disposition::Unverifiable => self.unverifiable += 1,
        }
    }

    /// Total pairs seen.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.pass + self.mismatch + self.discontinuous + self.unverifiable
    }

    /// Pass rate over pairs that were actually checkable.
    #[must_use]
    pub fn pass_rate(&self) -> Option<f64> {
        let checked = self.pass + self.mismatch;
        (checked > 0).then(|| self.pass as f64 / checked as f64)
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;

    #[test]
    fn a_clean_buy_then_observed_state_passes() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        let (d, ds, dt) = verify_transition(
            pre,
            100,
            45_000,
            true,
            PoolReserves {
                sol: 1_100,
                tok: 455_000,
            },
        );
        assert_eq!((d, ds, dt), (Disposition::Pass, 0, 0));
    }

    /// The signature we are hunting: right amounts, next state short by the
    /// fee on the SOL side. It must read as Mismatch with the fee as residual,
    /// never as Pass and never as Discontinuous.
    #[test]
    fn a_fee_leaving_the_pool_reads_as_a_mismatch_carrying_the_fee() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        let fee = 30;
        let (d, ds, dt) = verify_transition(
            pre,
            100,
            45_000,
            true,
            PoolReserves {
                sol: 1_100 - fee,
                tok: 455_000,
            },
        );
        assert_eq!(d, Disposition::Mismatch);
        assert_eq!((ds, dt), (i128::from(fee), 0), "residual names the fee");
    }

    /// A reserve moving against the trade's own direction cannot be our
    /// arithmetic — that is an intervening event, and conflating it with a
    /// math error is what makes a pass rate meaningless.
    #[test]
    fn a_reserve_moving_the_wrong_way_is_discontinuous_not_a_mismatch() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        // SOL was the input, so the SOL reserve must not shrink.
        let (d, _, _) = verify_transition(
            pre,
            100,
            45_000,
            true,
            PoolReserves {
                sol: 900,
                tok: 455_000,
            },
        );
        assert_eq!(d, Disposition::Discontinuous);
    }

    /// Orientation is not optional: a buy and a sell report their sides in
    /// opposite order, so chaining without normalising swaps the reserves.
    #[test]
    fn sides_normalise_to_a_fixed_orientation() {
        let buy = PoolReserves::from_sides(1_000, 500_000, true);
        let sell = PoolReserves::from_sides(500_000, 1_000, false);
        assert_eq!(buy, sell);
        assert_eq!(
            buy,
            PoolReserves {
                sol: 1_000,
                tok: 500_000
            }
        );
    }
}

/// Whether anything other than this trade moved the pool between two swaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Continuity {
    /// The fee-free side moved by exactly what the trade dictates. Nothing
    /// else touched the pool, so a residual on the *other* side is a clean
    /// measurement of our model rather than a mixture of model error and
    /// interference.
    Continuous,
    /// Something else moved the pool: an unrecorded swap, a liquidity change,
    /// a direct vault transfer, or a fee sweep.
    Intervened {
        /// What the trade alone requires the fee-free side to move by.
        expected: i128,
        /// What it actually moved by.
        observed: i128,
    },
    /// Both reserves moved the same direction, which no swap can do — the
    /// signature of a liquidity add or remove.
    LiquidityChanged { d_sol: i128, d_tok: i128 },
}

/// Continuity gate, deliberately **model-free**.
///
/// It asks only whether the *fee-free* side of the pool moved by exactly what
/// the trade says it must. For PumpSwap and pumpfun the fee is charged on the
/// quote (SOL) side, so the token side is fully determined by the executed
/// amounts with no routing ambiguity — which makes it an oracle for "did
/// anything else happen", independent of the quote math being graded.
///
/// This is what keeps the SOL-side residual meaningful. Gating on the same
/// side we are measuring would be circular: every disagreement would be
/// reclassified as interference and the pass rate would be vacuous.
#[must_use]
pub fn check_continuity(
    pre: PoolReserves,
    amount_in: u64,
    amount_out: u64,
    sol_is_input: bool,
    next: PoolReserves,
) -> Continuity {
    let d_sol = i128::from(next.sol) - i128::from(pre.sol);
    let d_tok = i128::from(next.tok) - i128::from(pre.tok);

    let expected = if sol_is_input {
        -i128::from(amount_out)
    } else {
        i128::from(amount_in)
    };

    // The fee-free side alone decides admissibility. This test must come
    // FIRST and must not consult `d_sol`: an earlier version ran the
    // same-sign liquidity check ahead of it, which reclassified a pair as a
    // liquidity event whenever the SOL reserve happened to fall — consulting
    // the very side the gate exists to protect. Its own test caught it.
    if d_tok == expected {
        return Continuity::Continuous;
    }

    // Only now, having established the pair is NOT admissible, is it safe to
    // look at both sides — this only labels *why*, it cannot gate anything.
    // A swap moves the two sides in opposite directions, always; same-sign
    // movement is a deposit or a withdrawal.
    if d_sol != 0 && d_tok != 0 && (d_sol > 0) == (d_tok > 0) {
        return Continuity::LiquidityChanged { d_sol, d_tok };
    }
    Continuity::Intervened {
        expected,
        observed: d_tok,
    }
}

#[cfg(test)]
mod continuity_tests {
    use super::*;

    #[test]
    fn the_fee_free_side_moving_exactly_as_the_trade_says_is_continuous() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        // Buy: token side must fall by exactly amount_out. SOL side is left
        // deliberately wrong to prove the gate ignores it.
        let next = PoolReserves {
            sol: 12_345,
            tok: 455_000,
        };
        assert_eq!(
            check_continuity(pre, 100, 45_000, true, next),
            Continuity::Continuous
        );
    }

    #[test]
    fn an_extra_swap_shows_up_on_the_fee_free_side() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        let next = PoolReserves {
            sol: 1_200,
            tok: 440_000,
        }; // 60k moved, not 45k
        assert!(matches!(
            check_continuity(pre, 100, 45_000, true, next),
            Continuity::Intervened {
                expected: -45_000,
                observed: -60_000
            }
        ));
    }

    /// Both reserves rising is a deposit, and must not be reported as a
    /// gigantic model error.
    #[test]
    fn both_reserves_rising_is_a_liquidity_event() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        let next = PoolReserves {
            sol: 2_000,
            tok: 1_000_000,
        };
        assert!(matches!(
            check_continuity(pre, 100, 45_000, true, next),
            Continuity::LiquidityChanged { .. }
        ));
    }

    /// The gate must not consult the side being measured — otherwise every
    /// SOL-side disagreement is reclassified as interference and the pass rate
    /// means nothing.
    #[test]
    fn the_gate_ignores_the_side_it_is_protecting() {
        let pre = PoolReserves {
            sol: 1_000,
            tok: 500_000,
        };
        for sol in [0_u64, 1, 1_100, u64::MAX / 2] {
            assert_eq!(
                check_continuity(pre, 100, 45_000, true, PoolReserves { sol, tok: 455_000 }),
                Continuity::Continuous,
                "sol={sol} changed the verdict"
            );
        }
    }
}
