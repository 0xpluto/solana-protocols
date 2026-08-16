//! Every instruction's log slice must be its own.
//!
//! `parse_instructions` splits a transaction's flat log stream across its
//! instructions by walking both in lockstep. The walk is positional, so one
//! mis-step shifts every later window, and nothing about a wrong window looks
//! wrong: an instruction still has *a* list of logs. Anything that reads those
//! logs — `Program data:` events, compute attribution, failure decoding —
//! inherits the error silently.
//!
//! The fixtures under `fixtures/logs/` are whole mainnet transactions harvested
//! from the firehose (`CAPTURE_LOG_FIXTURES=<dir>` on talond), one per
//! (distinct programs, max CPI depth) shape. Hand-written streams were
//! deliberately not used: they would encode what we believe the runtime emits,
//! and this test exists because that belief was wrong.
//!
//! Single-program and un-nested transactions are excluded at capture time —
//! they satisfy the invariant even when attribution is completely broken, so
//! they are not evidence.

use std::collections::BTreeMap;
use std::path::PathBuf;

use solana_protocols::parsing::{audit_log_slice, LogFixture, LogSliceVerdict};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("logs")
}

/// Collapse a verdict to a stable label. The payloads (which foreign program,
/// which depth) are useful in the failure report but would fragment the tally.
fn label(v: &LogSliceVerdict) -> &'static str {
    match v {
        LogSliceVerdict::Ok => "ok",
        LogSliceVerdict::Truncated => "truncated",
        LogSliceVerdict::Empty => "empty",
        LogSliceVerdict::ForeignOpen { .. } => "foreign_open",
        LogSliceVerdict::NotOpened => "not_opened",
        LogSliceVerdict::DepthMismatch { .. } => "depth_mismatch",
        LogSliceVerdict::InteriorInvoke => "interior_invoke",
        LogSliceVerdict::Unterminated => "unterminated",
    }
}

#[test]
fn every_instruction_owns_its_log_slice() {
    let fixtures = LogFixture::load_dir(&fixture_dir()).expect("fixtures load");
    assert!(
        !fixtures.is_empty(),
        "no fixtures in {} — a green run over an empty corpus proves nothing",
        fixture_dir().display()
    );

    let mut tally: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut total = 0usize;

    for (name, fixture) in &fixtures {
        let parsed = fixture.replay().expect("replay");
        assert_eq!(
            parsed.len(),
            fixture.instructions.len(),
            "{name}: replay dropped instructions"
        );
        for ix in &parsed {
            total += 1;
            let verdict = audit_log_slice(ix);
            *tally.entry(label(&verdict)).or_default() += 1;
            if matches!(verdict, LogSliceVerdict::Ok | LogSliceVerdict::Truncated) {
                continue;
            }
            if failures.len() < 12 {
                failures.push(format!(
                    "  {name} ix#{} {} depth {} -> {verdict:?}\n    first log: {:?}",
                    ix.instruction_index,
                    ix.program_id,
                    ix.stack_height,
                    ix.logs.first()
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "log slices are misattributed\n\
             {} fixtures, {total} instructions\n\
             tally: {tally:?}\n\
             first offenders:\n{}",
            fixtures.len(),
            failures.join("\n")
        );
    }
}

/// A parent's slice must never swallow a child's `Invoke`, and a child's slice
/// must never start before its own. Stated as a whole-transaction property:
/// each `Invoke` line in the stream lands in exactly one instruction's slice,
/// as that slice's first entry.
#[test]
fn each_invoke_line_opens_exactly_one_slice() {
    let fixtures = LogFixture::load_dir(&fixture_dir()).expect("fixtures load");
    let mut wrong = Vec::new();

    for (name, fixture) in &fixtures {
        let invokes_in_stream = fixture
            .logs
            .iter()
            .filter(|l| l.contains(" invoke ["))
            .count();
        let parsed = fixture.replay().expect("replay");
        let truncated = parsed.iter().any(|ix| ix.logs_truncated());
        if truncated {
            continue; // the stream is incomplete; counts cannot balance
        }
        let opening: usize = parsed
            .iter()
            .filter(|ix| {
                matches!(
                    ix.logs.first(),
                    Some(solana_protocols::parsing::LogEntry::Invoke { .. })
                )
            })
            .count();
        if opening != invokes_in_stream {
            wrong.push(format!(
                "  {name}: {invokes_in_stream} invoke lines, {opening} slices opened by one"
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "invoke lines and slice openings do not balance:\n{}",
        wrong.join("\n")
    );
}
