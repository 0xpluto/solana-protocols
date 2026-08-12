//! Level 2 of the swap verifier: does our state model land on the *next*
//! swap's independently observed state?
//!
//! One pair = two swaps consecutive on the same pool. We take the first swap's
//! observed state, apply its trade, and compare against the second swap's
//! observed state. Nothing is simulated and nothing is invented: both endpoints
//! are facts the chain published, and the arithmetic between them is ours.
//!
//! The chaining direction is **per protocol**, driven by which side of the
//! trade that protocol's event publishes — PumpSwap publishes pre-swap
//! reserves, pumpfun publishes post-swap. That is the whole reason `Swap`
//! carries `state_before`/`state_after` as separate `Option`s rather than one
//! field with a convention attached.
//!
//! Run against a live tape:
//! ```text
//! VERIFY_PAIRS_PATH=/path/pairs.json cargo test -p solana-protocols \
//!     --test verify_pairs -- --nocapture
//! ```
//! Without it the test is inert — see `docs/solana-swap-programs.md` §6.

use std::collections::BTreeMap;

use solana_protocols::verify::{
    check_continuity, verify_transition, Continuity, Disposition, PairTally, PoolReserves,
};

#[derive(serde::Deserialize)]
struct Pair {
    protocol: String,
    pre_in: u64,
    pre_out: u64,
    pre_sol_is_input: bool,
    amount_in: u64,
    amount_out: u64,
    trade_sol_is_input: bool,
    fee_amount: u64,
    next_in: u64,
    next_out: u64,
    next_sol_is_input: bool,
}

fn report(label: &str, t: &PairTally) {
    eprintln!(
        "{label:22} pairs {:6}  pass {:6}  mismatch {:6}  discontinuous {:6}  unverifiable {:5}  => {}",
        t.total(),
        t.pass,
        t.mismatch,
        t.discontinuous,
        t.unverifiable,
        t.pass_rate()
            .map_or_else(|| "n/a".to_string(), |r| format!("{:.1}% of checkable", r * 100.0))
    );
}

#[test]
fn state_transition_verified_against_consecutive_swaps() {
    let Ok(path) = std::env::var("VERIFY_PAIRS_PATH") else {
        eprintln!("VERIFY_PAIRS_PATH unset — inert. See docs/solana-swap-programs.md §6.");
        return;
    };
    let pairs: Vec<Pair> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("pairs readable"))
            .expect("pairs parse");
    eprintln!("sample: {} pairs from {path}\n", pairs.len());

    let mut by_proto: BTreeMap<&str, PairTally> = BTreeMap::new();
    // Residual structure is the diagnostic, not the pass rate: a residual that
    // equals the fee names fee routing, one that scales with the trade names
    // the curve, one that scales with the reserves names orientation.
    let mut residual_is_fee = 0u64;
    let mut residual_is_neg_fee = 0u64;
    let mut residual_other: Vec<(i128, i128)> = Vec::new();
    let mut cont_reason: BTreeMap<&str, u64> = BTreeMap::new();

    for p in &pairs {
        let t = by_proto.entry(p.protocol.as_str()).or_default();
        let pre = PoolReserves::from_sides(p.pre_in, p.pre_out, p.pre_sol_is_input);
        let next = PoolReserves::from_sides(p.next_in, p.next_out, p.next_sol_is_input);

        // Admissibility first, on the fee-free side only. A pair the pool
        // moved underneath cannot grade our model either way, and counting it
        // as a mismatch would blame our math for someone else's liquidity.
        let cont = check_continuity(pre, p.amount_in, p.amount_out, p.trade_sol_is_input, next);
        *cont_reason
            .entry(match cont {
                Continuity::Continuous => "continuous",
                Continuity::Intervened { .. } => "intervened",
                Continuity::LiquidityChanged { .. } => "liquidity changed",
            })
            .or_default() += 1;
        if cont != Continuity::Continuous {
            t.record(Disposition::Discontinuous);
            continue;
        }

        let (d, d_sol, d_tok) =
            verify_transition(pre, p.amount_in, p.amount_out, p.trade_sol_is_input, next);
        t.record(d);

        if d == Disposition::Mismatch {
            let fee = i128::from(p.fee_amount);
            if d_tok == 0 && d_sol == fee && fee != 0 {
                residual_is_fee += 1;
            } else if d_tok == 0 && d_sol == -fee && fee != 0 {
                residual_is_neg_fee += 1;
            } else if residual_other.len() < 12 {
                residual_other.push((d_sol, d_tok));
            }
        }
    }

    let mut all = PairTally::default();
    for (proto, t) in &by_proto {
        report(proto, t);
        all.pass += t.pass;
        all.mismatch += t.mismatch;
        all.discontinuous += t.discontinuous;
        all.unverifiable += t.unverifiable;
    }
    report("ALL", &all);

    eprintln!("\n-- continuity gate (fee-free side) --");
    for (k, v) in &cont_reason {
        eprintln!("  {k:20} {v}");
    }

    eprintln!("\n-- mismatch residual structure (predicted - observed) --");
    eprintln!("  SOL side == +fee, token side 0 : {residual_is_fee}  (fee LEFT the pool)");
    eprintln!("  SOL side == -fee, token side 0 : {residual_is_neg_fee}  (fee counted twice)");
    eprintln!("  other, first few               : {residual_other:?}");

    assert_eq!(
        all.total(),
        pairs.len() as u64,
        "every pair must reach a disposition"
    );
}
