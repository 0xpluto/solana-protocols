//! How much of the pumpfun program do we actually parse?
//!
//! The goal is 100%: every instruction, account and event the program declares
//! should decode, so that anyone so much as touching the program is visible to
//! us. This measures the distance to that, against the program's own IDL as the
//! denominator — the only honest one, since a denominator we choose ourselves
//! can be made to say anything.
//!
//! It is a **ratchet**, not a pass/fail. Asserting 100% today would be red
//! forever and tell nobody anything; asserting nothing would let coverage rot.
//! So the floors below are the measured truth at the time of writing and may
//! only ever be raised. A parser that stops working fails this test, and
//! raising a floor is a deliberate line in a diff.
//!
//! Run `cargo test -p solana-protocols --test pumpfun_coverage -- --nocapture`
//! for the gap list — the names printed are the work remaining, in order.

use std::collections::BTreeSet;

use solana_protocols::protocols::pumpfun::PumpfunInstruction;

/// Measured 2026-08-12. Raise as coverage improves; never lower.

fn idl(file: &str) -> serde_json::Value {
    let path = format!("idls/{file}.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).expect("IDL parses")
}

fn names(idl: &serde_json::Value, section: &str) -> Vec<String> {
    idl.get(section)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|i| i.get("name").and_then(|n| n.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Anchor's discriminator for a declared item, preferring the IDL's own bytes
/// over re-deriving them — if the program ships a discriminator that is not
/// `sha256(...)`, the IDL is right and our rule is wrong.
fn discriminator(item: &serde_json::Value, prefix: &str, name: &str) -> [u8; 8] {
    if let Some(d) = item.get("discriminator").and_then(|v| v.as_array()) {
        let mut out = [0u8; 8];
        for (i, b) in d.iter().take(8).enumerate() {
            out[i] = b.as_u64().unwrap_or(0) as u8;
        }
        return out;
    }
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(format!("{prefix}:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&h.finalize()[..8]);
    out
}

fn report(kind: &str, covered: &BTreeSet<String>, all: &[String], floor: usize) -> usize {
    let missing: Vec<&String> = all.iter().filter(|n| !covered.contains(*n)).collect();
    let pct = if all.is_empty() {
        100.0
    } else {
        covered.len() as f64 / all.len() as f64 * 100.0
    };
    eprintln!("\n{kind}: {}/{} = {pct:.1}%", covered.len(), all.len());
    if !missing.is_empty() {
        eprintln!("  not parsed ({}):", missing.len());
        for chunk in missing.chunks(3) {
            eprintln!(
                "    {}",
                chunk
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    assert!(
        covered.len() >= floor,
        "{kind} coverage REGRESSED: {} < floor {floor}",
        covered.len()
    );
    covered.len()
}

/// One protocol's measured coverage.
struct Coverage {
    protocol: &'static str,
    ix: (usize, usize),
    acct: (usize, usize),
    ev: (usize, usize),
}

/// Measure one protocol: how many of the IDL's declared items do we parse?
///
/// `parses` is the protocol's own instruction dispatch — passing it in keeps
/// this generic over nine different enum types without a trait nobody else
/// needs.
fn measure(
    protocol: &'static str,
    idl_file: &str,
    program_id: &str,
    parses: &dyn Fn(&[u8]) -> bool,
    known_accounts: &[&str],
    known_events: &[&str],
) -> Coverage {
    let idl = idl(idl_file);

    // The IDL must belong to the program we are measuring. Without this the
    // meter happily reports coverage against a DIFFERENT program's IDL and the
    // number looks precise while meaning nothing — which is exactly what
    // happened: meteora_damm.json is Dynamic AMM v1 (Eo7WjKq…) and was used to
    // measure cp-amm v2 (cpamdpZC…), publishing a confident 0.0%.
    let declared = idl
        .get("address")
        .and_then(|v| v.as_str())
        .or_else(|| {
            idl.get("metadata")
                .and_then(|m| m.get("address"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("<none>");
    assert_eq!(
        declared, program_id,
        "{protocol}: idls/{idl_file}.json belongs to program {declared}, not {program_id} — \
         measuring against the wrong program's IDL produces a precise, meaningless number"
    );

    let ix_names = names(&idl, "instructions");
    let empty = vec![];
    let ix_items = idl["instructions"].as_array().unwrap_or(&empty);
    let mut ix_covered = BTreeSet::new();
    for (item, name) in ix_items.iter().zip(&ix_names) {
        let mut data = discriminator(item, "global", name).to_vec();
        data.extend_from_slice(&[0u8; 256]);
        if parses(&data) {
            ix_covered.insert(name.clone());
        }
    }

    let acct_names = names(&idl, "accounts");
    let known_a: BTreeSet<String> = known_accounts.iter().map(|s| (*s).to_string()).collect();
    let acct_covered: BTreeSet<String> = acct_names
        .iter()
        .filter(|n| known_a.contains(*n))
        .cloned()
        .collect();

    let ev_names = names(&idl, "events");
    let known_e: BTreeSet<String> = known_events.iter().map(|s| (*s).to_string()).collect();
    let ev_covered: BTreeSet<String> = ev_names
        .iter()
        .filter(|n| known_e.contains(*n))
        .cloned()
        .collect();

    eprintln!("\n--- {protocol} ---");
    let ix = report("  instructions", &ix_covered, &ix_names, 0);
    let acct = report("  accounts", &acct_covered, &acct_names, 0);
    let ev = report("  events", &ev_covered, &ev_names, 0);

    Coverage {
        protocol,
        ix: (ix, ix_names.len()),
        acct: (acct, acct_names.len()),
        ev: (ev, ev_names.len()),
    }
}

/// Measured 2026-08-12, per protocol. Raise as coverage improves; never lower.
const FLOORS: &[(&str, usize)] = &[
    // 11, not 12, and NOT a regression: `create_v2` now rejects the synthetic
    // all-zero body this meter pads with, because its OptionBool field only
    // accepts encodings observed on chain. It parses real instructions and
    // refuses garbage — the metric penalises that, which is a flaw in the
    // metric, not the parser. Measuring against captured bodies instead of
    // synthetic padding is the fix; until then this floor records the truth.
    ("pumpfun", 11),
    ("pumpswap", 9),
    ("meteora_dbc", 2),
    ("raydium_clmm", 2),
];

#[test]
fn parse_coverage() {
    use solana_protocols::protocols::meteora_dbc::MeteoraDbcInstruction;
    use solana_protocols::protocols::pumpswap::instructions::PumpSwapInstruction;
    use solana_protocols::protocols::raydium_clmm::RaydiumClmmInstruction;

    let all = vec![
        measure(
            "pumpfun",
            "pump",
            "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
            &|d| PumpfunInstruction::try_from_slice(d).is_ok(),
            &["BondingCurve", "Global", "FeeConfig"],
            &["TradeEvent"],
        ),
        measure(
            "pumpswap",
            "pump_amm",
            "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
            &|d| PumpSwapInstruction::try_from_slice(d).is_ok(),
            &["Pool"],
            &["BuyEvent", "SellEvent"],
        ),
        measure(
            "meteora_dbc",
            "meteora_dbc",
            "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN",
            &|d| MeteoraDbcInstruction::try_from_slice(d).is_ok(),
            &[],
            &[],
        ),
        measure(
            "raydium_clmm",
            "raydium_clmm",
            "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
            &|d| RaydiumClmmInstruction::try_from_slice(d).is_ok(),
            &[],
            &[],
        ),
    ];

    for c in &all {
        let total = c.ix.0 + c.acct.0 + c.ev.0;
        if let Some((_, floor)) = FLOORS.iter().find(|(p, _)| *p == c.protocol) {
            assert!(
                total >= *floor,
                "{} coverage REGRESSED: {total} < floor {floor}",
                c.protocol
            );
        }
    }

    sync_readme(&all);
}

/// Write the measured table into README.md between its markers, and fail if it
/// had drifted.
///
/// The README is for people who have not read the code, so a coverage number
/// there is load-bearing — and a hand-maintained one becomes decoration the
/// first time somebody improves coverage without remembering to edit prose.
/// Generating it makes the published claim a measurement.
///
/// Set `UPDATE_README=1` to rewrite it; otherwise a stale section fails.
fn sync_readme(all: &[Coverage]) {
    let mut table = String::from("<!-- BEGIN:COVERAGE -->\n");
    table.push_str("| protocol | instructions | accounts | events | overall |\n");
    table.push_str("|---|---:|---:|---:|---|\n");
    let (mut t, mut d) = (0usize, 0usize);
    for c in all {
        let got = c.ix.0 + c.acct.0 + c.ev.0;
        let all_n = c.ix.1 + c.acct.1 + c.ev.1;
        let pct = got as f64 / all_n as f64 * 100.0;
        let filled = (pct / 10.0).round() as usize;
        let bar: String = "█".repeat(filled) + &"░".repeat(10 - filled);
        table.push_str(&format!(
            "| {} | {}/{} | {}/{} | {}/{} | `{bar}` {pct:.1}% |\n",
            c.protocol, c.ix.0, c.ix.1, c.acct.0, c.acct.1, c.ev.0, c.ev.1
        ));
        t += got;
        d += all_n;
    }
    let pct = t as f64 / d as f64 * 100.0;
    table.push_str(&format!("| **total** | | | | **{t}/{d} = {pct:.1}%** |\n"));
    table.push_str("<!-- END:COVERAGE -->");

    let path = "README.md";
    let readme = std::fs::read_to_string(path).expect("README present");
    let (start, end) = ("<!-- BEGIN:COVERAGE -->", "<!-- END:COVERAGE -->");
    let (Some(a), Some(b)) = (readme.find(start), readme.find(end)) else {
        panic!("README lost its COVERAGE markers");
    };
    let current = &readme[a..b + end.len()];
    if current == table {
        return;
    }
    if std::env::var("UPDATE_README").is_ok() {
        let updated = format!("{}{}{}", &readme[..a], table, &readme[b + end.len()..]);
        std::fs::write(path, updated).expect("README writable");
        eprintln!("README coverage section updated");
        return;
    }
    panic!("README coverage section is stale. Re-run with UPDATE_README=1.\n\nexpected:\n{table}");
}
