//! What our stack can actually do with every instruction we have witnessed.
//!
//! Not a pass/fail gate on individual instructions — a *measurement*, printed as
//! a matrix, over every fixture captured from the firehose. Each fixture is one
//! real landed instruction at one observed account count, so the matrix answers
//! the only question that matters before fixing anything: which shapes do we
//! decode, and which do we drop on the floor?
//!
//! Two assertions keep it from being decorative: the corpus must be non-empty,
//! and the totals must not regress below a committed baseline. A measurement
//! that cannot fail is a measurement nobody maintains.
//!
//! Run `cargo test -p solana-protocols --test instruction_coverage -- --nocapture`
//! to read the matrix.

use std::collections::BTreeMap;

use solana_program::pubkey::Pubkey;
use solana_protocols::protocols::pumpfun::PumpfunInstruction;
use solana_protocols::protocols::pumpswap::PumpSwapInstruction;

const PUMPFUN: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const PUMPSWAP: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

/// What happened to one fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Params decoded and the account list parsed into a typed struct.
    Full,
    /// Params decoded; the instruction declares no accounts struct.
    ParamsOnly,
    /// Params decoded; the account list was refused.
    AccountsRefused,
    /// The instruction data itself would not decode.
    ParamsRefused,
    /// The discriminator is not one we model.
    Unknown,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::ParamsOnly => "params-only",
            Self::AccountsRefused => "accounts REFUSED",
            Self::ParamsRefused => "params REFUSED",
            Self::Unknown => "unknown discriminator",
        }
    }
}

struct Row {
    protocol: &'static str,
    instruction: String,
    declared: usize,
    actual: usize,
    verdict: Verdict,
    detail: String,
}

fn fixtures() -> Vec<serde_json::Value> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut out = Vec::new();
    for proto in ["pumpfun", "pumpswap"] {
        let dir = root.join(proto);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if !name.starts_with("ix_") || p.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let raw = std::fs::read_to_string(&p).expect("fixture readable");
            let mut v: serde_json::Value = serde_json::from_str(&raw).expect("fixture parses");
            v["__file"] = serde_json::Value::String(format!("{proto}/{name}"));
            out.push(v);
        }
    }
    out
}

fn keys(v: &serde_json::Value) -> Vec<Pubkey> {
    use std::str::FromStr;
    v["accounts"]
        .as_array()
        .expect("accounts")
        .iter()
        .map(|a| Pubkey::from_str(a["pubkey"].as_str().expect("pubkey")).expect("valid"))
        .collect()
}

fn data(v: &serde_json::Value) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(v["data_b64"].as_str().expect("data_b64"))
        .expect("base64")
}

fn judge(v: &serde_json::Value) -> Row {
    let program = v["program"].as_str().unwrap_or_default();
    let d = data(v);
    let k = keys(v);
    let declared = v["declared_accounts"].as_u64().unwrap_or(0) as usize;
    let name = v["instruction"].as_str().unwrap_or("?").to_string();

    let (protocol, params, accounts) = match program {
        PUMPFUN => {
            let ix = PumpfunInstruction::try_from_slice(&d);
            let acc = ix.as_ref().ok().map(|i| i.from_accounts(&k).map(|_| ()));
            ("pumpfun", ix.map(|_| ()).map_err(|e| e.to_string()), acc)
        }
        PUMPSWAP => {
            let ix = PumpSwapInstruction::try_from_slice(&d);
            let acc = ix.as_ref().ok().map(|i| i.from_accounts(&k).map(|_| ()));
            ("pumpswap", ix.map(|_| ()).map_err(|e| e.to_string()), acc)
        }
        other => panic!("fixture names an unexpected program: {other}"),
    };

    let (verdict, detail) = match (&params, &accounts) {
        (Err(e), _) if e.contains("nknown") => (Verdict::Unknown, e.clone()),
        (Err(e), _) => (Verdict::ParamsRefused, e.clone()),
        (Ok(()), Some(Ok(()))) => (Verdict::Full, String::new()),
        (Ok(()), Some(Err(e))) => (Verdict::AccountsRefused, e.to_string()),
        (Ok(()), None) => (Verdict::ParamsOnly, String::new()),
    };
    Row {
        protocol,
        instruction: name,
        declared,
        actual: k.len(),
        verdict,
        detail,
    }
}

/// The matrix, plus a floor so it cannot silently regress.
#[test]
fn instruction_coverage_over_every_witnessed_shape() {
    let fx = fixtures();
    assert!(
        fx.len() >= 40,
        "only {} fixtures — the corpus is the measurement, so an empty one is a \
         broken test rather than perfect coverage",
        fx.len()
    );

    let mut rows: Vec<Row> = fx.iter().map(judge).collect();
    rows.sort_by(|a, b| {
        (a.protocol, &a.instruction, a.actual).cmp(&(b.protocol, &b.instruction, b.actual))
    });

    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    println!("\n=== instruction coverage over {} witnessed shapes ===\n", rows.len());
    println!(
        "{:<10} {:<30} {:>5} {:>5}  verdict",
        "protocol", "instruction", "decl", "seen"
    );
    for r in &rows {
        *tally.entry(r.verdict.label()).or_default() += 1;
        let tail = r.actual as i64 - r.declared as i64;
        println!(
            "{:<10} {:<30} {:>5} {:>5}  {:<22} {}{}",
            r.protocol,
            r.instruction,
            r.declared,
            r.actual,
            r.verdict.label(),
            if tail > 0 { format!("tail={tail} ") } else { String::new() },
            r.detail,
        );
    }

    println!("\n--- totals ---");
    for (k, v) in &tally {
        println!("  {k:<22} {v}");
    }
    let full = tally.get("full").copied().unwrap_or(0);
    println!(
        "\n  decoded end to end: {full}/{} = {:.0}%",
        rows.len(),
        100.0 * full as f64 / rows.len() as f64
    );

    // Ratchet, set to where we measured. Raise on improvement; a drop is a
    // failing test rather than a number nobody re-read.
    const BASELINE_FULL: usize = 36;
    assert!(
        full >= BASELINE_FULL,
        "end-to-end coverage fell to {full}, baseline is {BASELINE_FULL}"
    );

    // Params refusals are a different class from account gaps: the instruction
    // *data* will not decode, which is a layout bug on a path that already
    // claims to work. Pinned at the measured count so closing one is visible and
    // introducing one fails here.
    // 16 → 36 once every appended-account tail was modelled: the accounts axis
    // went to zero refusals, so what is left is the params axis alone.
    //
    // Rose from 2 to 4 when pumpfun's `SellParams` stopped hand-rolling its own
    // decoder. The defects did not appear — a `data.len() < 16` *minimum* check
    // had been accepting instructions with trailing bytes and discarding them.
    // A count that goes up because the instrument improved is the instrument
    // working; it may only fall from here.
    const BASELINE_PARAMS_REFUSED: usize = 4;
    let refused = tally.get("params REFUSED").copied().unwrap_or(0);
    assert!(
        refused <= BASELINE_PARAMS_REFUSED,
        "params refusals rose to {refused} (baseline {BASELINE_PARAMS_REFUSED}): an \
         instruction whose data will not decode is a layout bug, never a gap"
    );
}
