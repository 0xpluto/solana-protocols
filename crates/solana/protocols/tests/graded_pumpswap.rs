//! (Requires the `cache-handlers` feature: the quote assembles from a cache.)
#![cfg(feature = "cache-handlers")]
//! Grade PumpSwap against swaps that actually executed.
//!
//! Real swaps from the live firehose, each carrying the reserves the protocol's
//! own event published *before* the swap, the amounts that moved, the fee the
//! chain charged, and whether the pool has a coin creator. Quotes run through
//! `PumpSwapQuote::assemble` — the production path — against a cache that
//! serves the recorded state.
//!
//! Two grades, because a quote is a curve walk plus a fee deduction and either
//! can be wrong alone:
//!
//! * [`Grade::curve`] — zero fee rates, compared against the pre-fee amount
//!   reconstructed from the fee the chain reported. Fails only if the curve is
//!   wrong.
//! * [`Grade::full`] — the real on-chain `FeeConfig`, so it grades the curve
//!   *and* fee selection, which is what a caller actually gets.
//!
//! # What earlier runs of this established (2026-08-10)
//!
//! The curve is not the problem. Misses cluster per pool with zero mixed pools,
//! and within a miss pool the implied correction is a fixed multiplicative rate
//! held to five decimals across swaps spanning 2689x in size — a wrong curve
//! produces size-dependent error, a constant rate is a fee.
//!
//! Two fee defects came out of that and are fixed: `Fees::select` had dropped
//! the `has_creator` gate (pools with no creator owe the config's flat rates,
//! not a tier), and `calculate_fees` floored the protocol and lp components
//! where the chain ceils them, leaving the fee 2 units short.
//!
//! Still open: for pools that *do* have a creator, the tier key is market cap,
//! which the pool account cannot supply — see `Fees::TIER_KEY_UNSETTLED`.
//!
//! # Token-2022 is not a factor here (checked fixture-wide, 2026-08-10)
//!
//! Every mint behind these 126 pools was fetched and its extensions decoded:
//! 137 rows are Token-2022 and 463 plain SPL, and the only extensions present
//! anywhere are 18 (MetadataPointer) and 19 (TokenMetadata). **No
//! `TransferFeeConfig` on any mint**, so no swap here moves less than
//! `amount_in` into the pool — the hypothesis that would have explained a
//! systematic shortfall is dead, not merely untested.
//!
//! Token-2022 tracks `has_creator` rather than the grade: pump's creator-era
//! mints are T22, and all 459 no-creator rows are SPL — including all 130 of
//! their misses.
//!
//! Consequently this harness pins vaults to `TokenProgram::SplToken`. That is
//! a deliberate simplification, not an oversight: a vault's token program does
//! not enter the quote (a balance is a balance), and every row it grades is
//! SPL-backed on the side that matters. It *would* matter for building
//! instructions, where the program decides ATA derivation.

use std::collections::{BTreeMap, HashMap};

use solana_account_traits::{CacheGet, CacheSingleton};
use solana_program::pubkey::Pubkey;
use solana_protocols::protocols::pumpfun::{PumpfunFeeConfig, PumpfunFeeTier, PumpfunFees};
use solana_protocols::protocols::pumpswap::quote::PumpSwapQuote;
use solana_protocols::protocols::pumpswap::{PumpSwapFeeConfig, PumpSwapPool};
use solana_protocols::tokens::{TokenAccount, TokenProgram, TokenWithProgram, WSOL};
use solana_protocols::traits::SwapDirection;
use solana_protocols::verify::{FeeSide, Grade, ObservedSwap, Outcome, Tally, Verdict};

#[derive(serde::Deserialize)]
struct Row {
    #[serde(default)]
    platform: String,
    token_in: String,
    amount_in: u64,
    amount_out: u64,
    fee_amount: u64,
    fee_mint: String,
    reserve_in: u64,
    reserve_out: u64,
    has_creator: bool,
}

#[derive(serde::Deserialize)]
struct Tier {
    threshold: String,
    lp: u64,
    protocol: u64,
    creator: u64,
}

#[derive(serde::Deserialize)]
struct Flat {
    lp: u64,
    protocol: u64,
    creator: u64,
}

#[derive(serde::Deserialize)]
struct FeeCfg {
    flat: Flat,
    tiers: Vec<Tier>,
}

/// A cache serving one pool's recorded state, and either the real fee config or
/// zeroed rates.
///
/// Implements the same traits `LocalCache` does, so the replay reaches the
/// quote through `assemble` rather than a bypass constructor that could drift.
struct RecordedState {
    pool_key: Pubkey,
    pool: PumpSwapPool,
    vaults: HashMap<Pubkey, TokenAccount>,
    fees: PumpSwapFeeConfig,
}

impl CacheGet<Pubkey, PumpSwapPool> for RecordedState {
    fn get(&self, key: &Pubkey) -> Option<PumpSwapPool> {
        (*key == self.pool_key).then(|| self.pool.clone())
    }
    fn get_with_slot(&self, key: &Pubkey) -> Option<(PumpSwapPool, u64)> {
        CacheGet::<Pubkey, PumpSwapPool>::get(self, key).map(|v| (v, 0))
    }
    fn get_at_slot(&self, key: &Pubkey, _: u64) -> Option<PumpSwapPool> {
        CacheGet::<Pubkey, PumpSwapPool>::get(self, key)
    }
    fn get_at_slot_with_slot(&self, key: &Pubkey, _: u64) -> Option<(PumpSwapPool, u64)> {
        CacheGet::<Pubkey, PumpSwapPool>::get_with_slot(self, key)
    }
}

impl CacheGet<Pubkey, TokenAccount> for RecordedState {
    fn get(&self, key: &Pubkey) -> Option<TokenAccount> {
        self.vaults.get(key).cloned()
    }
    fn get_with_slot(&self, key: &Pubkey) -> Option<(TokenAccount, u64)> {
        CacheGet::<Pubkey, TokenAccount>::get(self, key).map(|v| (v, 0))
    }
    fn get_at_slot(&self, key: &Pubkey, _: u64) -> Option<TokenAccount> {
        CacheGet::<Pubkey, TokenAccount>::get(self, key)
    }
    fn get_at_slot_with_slot(&self, key: &Pubkey, _: u64) -> Option<(TokenAccount, u64)> {
        CacheGet::<Pubkey, TokenAccount>::get_with_slot(self, key)
    }
}

impl CacheSingleton<PumpSwapFeeConfig> for RecordedState {
    fn get(&self) -> Option<PumpSwapFeeConfig> {
        Some(self.fees.clone())
    }
    fn set(&self, _: PumpSwapFeeConfig) {}
}

fn zero_fees() -> PumpSwapFeeConfig {
    PumpSwapFeeConfig(PumpfunFeeConfig {
        bump: 0,
        admin: Pubkey::default(),
        flat_fees: PumpfunFees {
            lp_fee_bps: 0,
            protocol_fee_bps: 0,
            creator_fee_bps: 0,
        },
        fee_tiers: Vec::new(),
    })
}

fn real_fees(cfg: &FeeCfg) -> PumpSwapFeeConfig {
    PumpSwapFeeConfig(PumpfunFeeConfig {
        bump: 0,
        admin: Pubkey::default(),
        flat_fees: PumpfunFees {
            lp_fee_bps: cfg.flat.lp,
            protocol_fee_bps: cfg.flat.protocol,
            creator_fee_bps: cfg.flat.creator,
        },
        fee_tiers: cfg
            .tiers
            .iter()
            .map(|t| PumpfunFeeTier {
                market_cap_lamports_threshold: t.threshold.parse().expect("u128 threshold"),
                fees: PumpfunFees {
                    lp_fee_bps: t.lp,
                    protocol_fee_bps: t.protocol,
                    creator_fee_bps: t.creator,
                },
            })
            .collect(),
    })
}

fn vault(mint: Pubkey, balance: u64) -> TokenAccount {
    TokenAccount {
        token: TokenWithProgram {
            mint,
            program: TokenProgram::SplToken,
        },
        owner: Pubkey::new_unique(),
        balance,
    }
}

/// Rebuild the recorded pool state. The tape orients reserves to the swap's own
/// `token_in`/`token_out`; the pool stores base/quote. Either branch resolves
/// to the same `(reserve_in, reserve_out)` pair once `SwapMath` re-derives by
/// direction, so the assignment cancels — checked, not assumed.
fn recorded(row: &Row, fees: PumpSwapFeeConfig) -> (RecordedState, SwapDirection) {
    let is_buy = row.token_in == WSOL.to_string();
    let (base_reserve, quote_reserve) = if is_buy {
        (row.reserve_out, row.reserve_in)
    } else {
        (row.reserve_in, row.reserve_out)
    };
    let base_mint = Pubkey::new_unique();
    let base_vault = Pubkey::new_unique();
    let quote_vault = Pubkey::new_unique();
    let pool = PumpSwapPool {
        base_mint,
        quote_mint: WSOL,
        pool_base_token_account: base_vault,
        pool_quote_token_account: quote_vault,
        // Drives `has_creator()`, which selects flat rates vs a tier.
        coin_creator: if row.has_creator {
            Pubkey::new_unique()
        } else {
            Pubkey::default()
        },
        ..PumpSwapPool::default()
    };
    let vaults = HashMap::from([
        (base_vault, vault(base_mint, base_reserve)),
        (quote_vault, vault(WSOL, quote_reserve)),
    ]);
    (
        RecordedState {
            pool_key: Pubkey::new_unique(),
            pool,
            vaults,
            fees,
        },
        if is_buy {
            SwapDirection::Buy
        } else {
            SwapDirection::Sell
        },
    )
}

fn observed_from(row: &Row, direction: SwapDirection) -> ObservedSwap {
    ObservedSwap {
        direction,
        amount_in: row.amount_in,
        amount_out: row.amount_out,
        fee_amount: Some(row.fee_amount),
        // PumpSwap charges in the quote mint, so a buy pays on the input leg
        // and a sell on the output leg. Read from the data, not assumed.
        fee_side: if row.fee_mint == row.token_in {
            FeeSide::Input
        } else {
            FeeSide::Output
        },
    }
}

/// Trade size in **lamports on the SOL leg**, bucketed on a log scale.
///
/// It must be the SOL leg, not `amount_in`: for a buy that is lamports, for a
/// sell it is the token's own base units. Bucketing on `amount_in` mixes the
/// two, and since sells are the larger raw numbers the top bucket silently
/// becomes "sells" — a direction breakdown wearing a size label.
fn size_bucket(row: &Row) -> &'static str {
    let sol = if row.token_in == WSOL.to_string() {
        row.amount_in
    } else {
        row.amount_out
    };
    bucket_lamports(sol)
}

fn bucket_lamports(amount_in: u64) -> &'static str {
    match amount_in {
        0..=9_999_999 => "<0.01",
        10_000_000..=99_999_999 => "0.01-0.1",
        100_000_000..=999_999_999 => "0.1-1",
        1_000_000_000..=9_999_999_999 => "1-10",
        10_000_000_000..=99_999_999_999 => "10-100",
        _ => ">100",
    }
}

/// Relative error buckets — the user-facing question.
///
/// Bit-exactness is the right bar for *finding* a bug, but it is not the bar
/// for shipping a quote: what matters is whether the number handed to a trader
/// would have burned them. A quote 0.02% off is a good quote; a quote 40% off
/// is a liability. Reporting only "exact vs not" conflates the two and makes a
/// harmless rounding difference read like a broken quoter.
#[derive(Default, Clone)]
struct Buckets {
    exact: u64,
    under_01: u64,
    under_1: u64,
    under_5: u64,
    under_10: u64,
    over_10: u64,
    errs: Vec<f64>,
}

impl Buckets {
    fn record(&mut self, v: &Verdict) {
        let Outcome::Off {
            predicted,
            observed,
            ..
        } = &v.outcome
        else {
            if matches!(v.outcome, Outcome::Exact) {
                self.exact += 1;
            }
            return;
        };
        if *observed == 0 {
            self.over_10 += 1;
            return;
        }
        let err = (predicted.abs_diff(*observed) as f64 / *observed as f64) * 100.0;
        self.errs.push(err);
        match err {
            e if e < 0.1 => self.under_01 += 1,
            e if e < 1.0 => self.under_1 += 1,
            e if e < 5.0 => self.under_5 += 1,
            e if e < 10.0 => self.under_10 += 1,
            _ => self.over_10 += 1,
        }
    }

    fn total(&self) -> u64 {
        self.exact + self.under_01 + self.under_1 + self.under_5 + self.under_10 + self.over_10
    }

    /// Share of quotes a trader would have accepted: exact, or inside 0.1%.
    fn usable_rate(&self) -> f64 {
        let n = self.total();
        if n == 0 {
            return 0.0;
        }
        (self.exact + self.under_01) as f64 / n as f64 * 100.0
    }

    fn pct(&mut self, q: f64) -> f64 {
        if self.errs.is_empty() {
            return 0.0;
        }
        self.errs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        self.errs[((self.errs.len() - 1) as f64 * q) as usize]
    }
}

fn report_buckets(label: &str, b: &mut Buckets) {
    let (p50, p95, max) = (b.pct(0.5), b.pct(0.95), b.pct(1.0));
    eprintln!(
        "{label:20} n {:6} | exact {:6} <0.1% {:6} <1% {:5} <5% {:5} <10% {:4} >=10% {:5} | usable {:5.1}% | err p50 {:.3}% p95 {:.2}% max {:.1}%",
        b.total(), b.exact, b.under_01, b.under_1, b.under_5, b.under_10, b.over_10,
        b.usable_rate(), p50, p95, max
    );
}

fn report(label: &str, t: &Tally, n: usize) {
    println!(
        "{label}: graded {}/{n}  exact {}  rounding {}  fee-rate {}  structural {}  \
         refused {}  skipped {}  => {:.1}% exact",
        t.graded(),
        t.exact,
        t.rounding,
        t.fee_rate,
        t.structural,
        t.refused,
        t.skipped,
        t.exact_rate().unwrap_or(0.0) * 100.0
    );
}

#[test]
fn pumpswap_graded_against_executed_swaps() {
    // The committed fixture is the default so the check is reproducible from a
    // clean checkout. `GRADED_SWAPS_PATH` points it at a larger sample pulled
    // off the chain tape — same production math, more rows — without committing
    // a multi-megabyte JSON or recompiling.
    let src = std::env::var("GRADED_SWAPS_PATH").ok();
    let text = match &src {
        Some(path) => std::fs::read_to_string(path).expect("GRADED_SWAPS_PATH readable"),
        None => include_str!("../fixtures/pumpswap/graded_swaps.json").to_string(),
    };
    let rows: Vec<Row> = serde_json::from_str(&text).expect("swap fixture parses");
    eprintln!(
        "sample: {} rows from {}",
        rows.len(),
        src.as_deref()
            .unwrap_or("fixtures/pumpswap/graded_swaps.json")
    );
    let cfg: FeeCfg = serde_json::from_str(include_str!("../fixtures/pumpswap/fee_config.json"))
        .expect("fee config fixture parses");
    assert!(rows.len() >= 100, "a grade needs a real sample");

    let (mut curve, mut full) = (Tally::default(), Tally::default());
    let (mut flat_full, mut creator_full) = (Tally::default(), Tally::default());
    // Breakdowns: a single headline rate hides whether the error is
    // size-dependent (a wrong curve) or flat (a wrong fee), and whether one
    // caller's flow is special.
    let mut by_size: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut by_dir: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut by_platform: BTreeMap<String, Tally> = BTreeMap::new();
    let mut curve_by_size: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut curve_by_dir: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut buckets_by_dir: BTreeMap<&'static str, Buckets> = BTreeMap::new();
    let mut buckets_by_size: BTreeMap<&'static str, Buckets> = BTreeMap::new();
    let mut buckets_all = Buckets::default();
    let mut buckets_curve = Buckets::default();

    for row in &rows {
        let (zs, dir) = recorded(row, zero_fees());
        let obs0 = observed_from(row, dir);
        let q0 = PumpSwapQuote::assemble(&zs, &zs.pool_key, 0).expect("state assembles");
        curve.record(&Grade::curve(&q0, &obs0));

        let (rs, dir) = recorded(row, real_fees(&cfg));
        let obs = observed_from(row, dir);
        let q = PumpSwapQuote::assemble(&rs, &rs.pool_key, 0).expect("state assembles");
        let v = Grade::full(&q, &obs);
        full.record(&v);
        if row.has_creator {
            creator_full.record(&v);
        } else {
            flat_full.record(&v);
        }

        buckets_all.record(&v);
        buckets_curve.record(&Grade::curve(&q0, &obs0));
        let bucket = size_bucket(row);
        buckets_by_size.entry(bucket).or_default().record(&v);
        buckets_by_dir
            .entry(match dir {
                SwapDirection::Buy => "Buy",
                SwapDirection::Sell => "Sell",
            })
            .or_default()
            .record(&v);
        by_size.entry(bucket).or_default().record(&v);
        curve_by_size
            .entry(bucket)
            .or_default()
            .record(&Grade::curve(&q0, &obs0));
        curve_by_dir
            .entry(match dir {
                SwapDirection::Buy => "Buy",
                SwapDirection::Sell => "Sell",
            })
            .or_default()
            .record(&Grade::curve(&q0, &obs0));
        by_dir
            .entry(match dir {
                SwapDirection::Buy => "Buy",
                SwapDirection::Sell => "Sell",
            })
            .or_default()
            .record(&v);
        if !row.platform.is_empty() {
            by_platform
                .entry(row.platform.clone())
                .or_default()
                .record(&v);
        }
    }

    report("curve (zero fees)", &curve, rows.len());
    report("full  (real config)", &full, rows.len());
    report("  full, no-creator ", &flat_full, rows.len());
    report("  full, has-creator", &creator_full, rows.len());

    eprintln!("\n===== RELATIVE ERROR — the user-facing bar =====");
    report_buckets("curve (zero fees)", &mut buckets_curve);
    report_buckets("full (real config)", &mut buckets_all);
    eprintln!("\n-- relative error by direction --");
    for (k, b) in buckets_by_dir.iter_mut() {
        report_buckets(k, b);
    }
    eprintln!("\n-- relative error by SOL size --");
    for (k, b) in buckets_by_size.iter_mut() {
        report_buckets(k, b);
    }

    eprintln!("\n-- curve exactness by trade size (a wrong curve errs with size) --");
    for (k, t) in &curve_by_size {
        report(&format!("  curve {k:>12}"), t, t.graded() as usize);
    }
    eprintln!("\n-- full exactness by trade size --");
    for (k, t) in &by_size {
        report(&format!("  {k:>18}"), t, t.graded() as usize);
    }
    eprintln!("\n-- curve by direction (isolates fee-side from curve/reserve mapping) --");
    for (k, t) in &curve_by_dir {
        report(&format!("  curve {k:>12}"), t, t.graded() as usize);
    }
    eprintln!("\n-- by direction --");
    for (k, t) in &by_dir {
        report(&format!("  {k:>18}"), t, t.graded() as usize);
    }
    if !by_platform.is_empty() {
        eprintln!("\n-- by platform (does one caller's flow price differently?) --");
        let mut v: Vec<_> = by_platform.iter().collect();
        v.sort_by_key(|(_, t)| std::cmp::Reverse(t.graded()));
        for (k, t) in v.into_iter().take(8) {
            report(&format!("  {k:>18}"), t, t.graded() as usize);
        }
    }

    // Every row reaches a disposition: an accuracy figure whose skips are
    // invisible has an unknown denominator.
    for (label, t) in [("curve", &curve), ("full", &full)] {
        assert_eq!(
            t.graded() + t.refused + t.skipped,
            rows.len() as u64,
            "{label}: every row must reach a disposition"
        );
    }

    // No-creator pools are the case the fee model fully determines: flat rates
    // from the config, no market cap needed. They are the regression guard.
    let flat_rate = flat_full.exact_rate().expect("no-creator rows graded");
    assert!(
        flat_rate > 0.60,
        "no-creator exact rate regressed: {:.1}%",
        flat_rate * 100.0
    );
}
