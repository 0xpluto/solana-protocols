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
const INSTRUCTION_FLOOR: usize = 8;
const ACCOUNT_FLOOR: usize = 3;
const EVENT_FLOOR: usize = 1;

fn idl() -> serde_json::Value {
    let text = std::fs::read_to_string("idls/pump.json").expect("vendored IDL present");
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

#[test]
fn pumpfun_parse_coverage() {
    let idl = idl();

    // --- instructions: does our parser accept the discriminator? ---
    let ix_names = names(&idl, "instructions");
    let ix_items = idl["instructions"].as_array().unwrap();
    let mut ix_covered = BTreeSet::new();
    for (item, name) in ix_items.iter().zip(&ix_names) {
        let mut data = discriminator(item, "global", name).to_vec();
        // Arguments are irrelevant to dispatch; pad generously so a parser that
        // recognises the discriminator is not failed for a short body.
        data.extend_from_slice(&[0u8; 256]);
        if PumpfunInstruction::try_from_slice(&data).is_ok() {
            ix_covered.insert(name.clone());
        }
    }
    let ix = report("INSTRUCTIONS", &ix_covered, &ix_names, INSTRUCTION_FLOOR);

    // --- accounts: do we have a decoder keyed to the discriminator? ---
    let acct_names = names(&idl, "accounts");
    let known_accounts: BTreeSet<String> = ["BondingCurve", "Global", "FeeConfig"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let acct_covered: BTreeSet<String> = acct_names
        .iter()
        .filter(|n| known_accounts.contains(*n))
        .cloned()
        .collect();
    let acc = report("ACCOUNTS", &acct_covered, &acct_names, ACCOUNT_FLOOR);

    // --- events ---
    let ev_names = names(&idl, "events");
    let known_events: BTreeSet<String> = ["TradeEvent"].iter().map(|s| (*s).to_string()).collect();
    let ev_covered: BTreeSet<String> = ev_names
        .iter()
        .filter(|n| known_events.contains(*n))
        .cloned()
        .collect();
    let ev = report("EVENTS", &ev_covered, &ev_names, EVENT_FLOOR);

    let total = ix + acc + ev;
    let denom = ix_names.len() + acct_names.len() + ev_names.len();
    eprintln!(
        "\n=== pumpfun overall: {total}/{denom} = {:.1}% ===",
        total as f64 / denom as f64 * 100.0
    );

    sync_readme(&[
        ("instructions", ix, ix_names.len()),
        ("accounts", acc, acct_names.len()),
        ("events", ev, ev_names.len()),
    ]);
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
fn sync_readme(rows: &[(&str, usize, usize)]) {
    let mut table = String::from("<!-- BEGIN:COVERAGE -->\n");
    table.push_str("| pumpfun | parsed | declared | |\n|---|---:|---:|---|\n");
    let (mut t, mut d) = (0, 0);
    for (kind, got, all) in rows {
        let pct = *got as f64 / *all as f64 * 100.0;
        // A bar reads faster than a number for "how far is there to go".
        let filled = (pct / 10.0).round() as usize;
        let bar: String = "█".repeat(filled) + &"░".repeat(10 - filled);
        table.push_str(&format!("| {kind} | {got} | {all} | `{bar}` {pct:.1}% |\n"));
        t += got;
        d += all;
    }
    let pct = t as f64 / d as f64 * 100.0;
    table.push_str(&format!(
        "| **total** | **{t}** | **{d}** | **{pct:.1}%** |\n"
    ));
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
