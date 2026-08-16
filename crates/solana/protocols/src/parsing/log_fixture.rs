//! Log-attribution fixtures: harvest real transactions, replay them, audit the
//! slices.
//!
//! [`parse_instructions`](super::parse_instructions) hands each instruction a
//! slice of the transaction's log stream. Every consumer that reads a log
//! positionally — compute attribution, `Program data:` events, failure
//! decoding — is only as correct as that slice. The walk is positional, so a
//! single mis-step shifts *every* later instruction's window, and the damage is
//! silent: a slice with the wrong contents still looks like a slice.
//!
//! So the invariant gets stated once, here, and checked against transactions
//! the chain actually produced:
//!
//! > An instruction's log slice opens with **its own** `Invoke` at **its own**
//! > stack height, closes with that program's terminator, and contains no
//! > nested `Invoke` — a child's logs belong to the child.
//!
//! [`audit_log_slice`] returns a typed verdict per instruction rather than a
//! bool, because "wrong" has distinguishable shapes and the shape names the
//! bug: `ForeignOpen` is misattribution across programs, `DepthMismatch` is a
//! CPI-level mix-up, `Unterminated` is a truncated walk.
//!
//! Fixtures are captured from the live firehose ([`capture_fixture`], opt-in
//! via `CAPTURE_LOG_FIXTURES=<dir>`) rather than hand-written, because a
//! hand-written log stream encodes what we *believe* the runtime emits, and
//! this invariant exists precisely because that belief was wrong.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;

use super::instruction::{parse_instructions, ParsedInstruction, RawInstruction};
use super::log::LogEntry;

/// One instruction's identity, which is all the log walk consumes.
///
/// Accounts and data are deliberately absent: attribution is decided by
/// program and stack height alone, and a fixture that carried payloads would
/// invite someone to assert on them here instead of in a decoder test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureInstruction {
    /// Executing program, base58.
    pub program: String,
    /// CPI depth, 1 = top level.
    pub stack_height: u32,
}

/// A whole transaction's attribution inputs, exactly as the runtime reported
/// them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFixture {
    /// Transaction signature — the handle for going back to the chain.
    pub signature: String,
    /// Slot the transaction landed in.
    pub slot: u64,
    /// Instructions in execution order, outer and inner flattened.
    pub instructions: Vec<FixtureInstruction>,
    /// Raw `meta.log_messages`, unparsed and in order.
    pub logs: Vec<String>,
}

impl LogFixture {
    /// Replay the fixture through the real attribution walk.
    ///
    /// Programs are re-indexed into a synthetic account-key table because
    /// [`parse_instructions`] addresses programs by index; nothing else about
    /// the account list affects attribution.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] if a program string is not a valid pubkey, or
    /// if the fixture names more than 256 distinct programs (the index is a
    /// `u8`, matching the wire format).
    pub fn replay(&self) -> Result<Vec<ParsedInstruction>, FixtureError> {
        let mut keys: Vec<Pubkey> = Vec::new();
        let mut indices: Vec<u8> = Vec::with_capacity(self.instructions.len());
        for ix in &self.instructions {
            let program: Pubkey = ix
                .program
                .parse()
                .map_err(|_| FixtureError::BadProgram(ix.program.clone()))?;
            let idx = match keys.iter().position(|k| *k == program) {
                Some(i) => i,
                None => {
                    keys.push(program);
                    keys.len() - 1
                }
            };
            indices.push(u8::try_from(idx).map_err(|_| FixtureError::TooManyPrograms)?);
        }

        let flat: Vec<RawInstruction<'_>> = self
            .instructions
            .iter()
            .zip(&indices)
            .map(|(ix, &idx)| (idx, [].as_slice(), Vec::new(), Some(ix.stack_height)))
            .collect();

        Ok(parse_instructions(
            flat.into_iter(),
            self.logs.iter().map(String::as_str),
            &keys,
        ))
    }

    /// Load every `*.json` fixture in `dir`, sorted by filename.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] if the directory cannot be read or a file does
    /// not deserialize.
    pub fn load_dir(dir: &Path) -> Result<Vec<(String, Self)>, FixtureError> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(|e| FixtureError::Io(e.to_string()))? {
            let path = entry.map_err(|e| FixtureError::Io(e.to_string()))?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let text =
                std::fs::read_to_string(&path).map_err(|e| FixtureError::Io(e.to_string()))?;
            let fixture: Self = serde_json::from_str(&text)
                .map_err(|e| FixtureError::Decode(name.clone(), e.to_string()))?;
            out.push((name, fixture));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}

/// Why a fixture could not be used.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// A `program` field is not valid base58 pubkey.
    #[error("fixture names an invalid program: {0}")]
    BadProgram(String),
    /// More distinct programs than the `u8` program index can address.
    #[error("fixture names more than 256 distinct programs")]
    TooManyPrograms,
    /// Filesystem failure while reading fixtures.
    #[error("fixture io: {0}")]
    Io(String),
    /// A fixture file did not deserialize.
    #[error("fixture {0} did not decode: {1}")]
    Decode(String, String),
}

/// The state of one instruction's log slice.
///
/// Exhaustive by design: a new failure shape must be named here and handled at
/// every call site rather than folding into a catch-all "invalid".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogSliceVerdict {
    /// Opens with its own `Invoke` at its own depth, closes with its own
    /// terminator, no child `Invoke` inside.
    Ok,
    /// The runtime cut the log stream off. Nothing downstream of the cut can be
    /// verified, and nothing is wrong with the walk.
    Truncated,
    /// No logs at all. Legitimate only when the whole stream was suppressed;
    /// otherwise the walk consumed this instruction's window into a sibling.
    Empty,
    /// Opens with an `Invoke` naming a *different* program: this slice belongs
    /// to somebody else.
    ForeignOpen {
        /// Program the opening `Invoke` actually names.
        found: Pubkey,
    },
    /// Opens with something other than an `Invoke` — a leftover `Success`,
    /// `Consumed` or message from the previous instruction, i.e. the walk is
    /// running behind.
    NotOpened,
    /// Opens with its own `Invoke`, but at the wrong CPI depth.
    DepthMismatch {
        /// Depth the opening `Invoke` reports.
        found: u32,
    },
    /// A nested `Invoke` sits inside the slice — a child's logs leaked into the
    /// parent.
    InteriorInvoke,
    /// Never reaches this program's `Success`/`failed` line.
    Unterminated,
}

/// Check one instruction's log slice against the bracket invariant.
///
/// Order matters: truncation is checked first because it makes every later
/// verdict unfalsifiable, then the opening entry, then the interior, then the
/// terminator. Reporting `Unterminated` for a slice that never opened correctly
/// would name the wrong defect.
#[must_use]
pub fn audit_log_slice(ix: &ParsedInstruction) -> LogSliceVerdict {
    if ix.logs.iter().any(|l| matches!(l, LogEntry::Truncated)) {
        return LogSliceVerdict::Truncated;
    }
    let Some(first) = ix.logs.first() else {
        return LogSliceVerdict::Empty;
    };
    match first {
        LogEntry::Invoke { program, depth } => {
            if *program != ix.program_id {
                return LogSliceVerdict::ForeignOpen { found: *program };
            }
            if *depth != ix.stack_height {
                return LogSliceVerdict::DepthMismatch { found: *depth };
            }
        }
        LogEntry::Success { .. }
        | LogEntry::Data { .. }
        | LogEntry::Return { .. }
        | LogEntry::Message { .. }
        | LogEntry::Consumed { .. }
        | LogEntry::Failed { .. }
        | LogEntry::Truncated
        | LogEntry::Unknown { .. } => return LogSliceVerdict::NotOpened,
    }

    if ix.logs[1..]
        .iter()
        .any(|l| matches!(l, LogEntry::Invoke { .. }))
    {
        return LogSliceVerdict::InteriorInvoke;
    }

    let terminated = ix.logs.last().is_some_and(|l| match l {
        LogEntry::Success { program } | LogEntry::Failed { program, .. } => {
            *program == ix.program_id
        }
        LogEntry::Invoke { .. }
        | LogEntry::Data { .. }
        | LogEntry::Return { .. }
        | LogEntry::Message { .. }
        | LogEntry::Consumed { .. }
        | LogEntry::Truncated
        | LogEntry::Unknown { .. } => false,
    });
    if terminated {
        LogSliceVerdict::Ok
    } else {
        LogSliceVerdict::Unterminated
    }
}

// ---------------------------------------------------------------------------
// Deep audit
// ---------------------------------------------------------------------------

/// Per-verdict tally, indexed by [`LogSliceVerdict`]'s discriminant order.
static VERDICTS: [AtomicU64; 8] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

fn verdict_slot(v: &LogSliceVerdict) -> usize {
    match v {
        LogSliceVerdict::Ok => 0,
        LogSliceVerdict::Truncated => 1,
        LogSliceVerdict::Empty => 2,
        LogSliceVerdict::ForeignOpen { .. } => 3,
        LogSliceVerdict::NotOpened => 4,
        LogSliceVerdict::DepthMismatch { .. } => 5,
        LogSliceVerdict::InteriorInvoke => 6,
        LogSliceVerdict::Unterminated => 7,
    }
}

/// Human label for each slot, in the same order.
pub const VERDICT_LABELS: [&str; 8] = [
    "ok",
    "truncated",
    "empty",
    "foreign_open",
    "not_opened",
    "depth_mismatch",
    "interior_invoke",
    "unterminated",
];

fn deep_audit_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AUDIT_LOG_SLICES").is_some())
}

/// Audit every instruction's slice against the full bracket invariant.
///
/// The always-on counters in [`attribution_stats`](super::attribution_stats)
/// only witness the *opening* of each slice — that the nth `invoke` named the
/// nth instruction's program at its depth. They say nothing about the interior
/// or the terminator, which the walk guarantees *structurally*; this is how
/// that guarantee gets checked against traffic instead of argued from the code.
///
/// Opt-in via `AUDIT_LOG_SLICES=1`, because it re-scans every slice and the
/// production answer to "is attribution healthy" is already free.
pub fn deep_audit(instructions: &[ParsedInstruction]) -> Option<&'static str> {
    if !deep_audit_enabled() {
        return None;
    }
    let mut imperfect = None;
    for ix in instructions {
        let slot = verdict_slot(&audit_log_slice(ix));
        VERDICTS[slot].fetch_add(1, Ordering::Relaxed);
        // Slots 0 and 1 are Ok and Truncated; the rest are ours to explain.
        if slot > 1 && imperfect.is_none() {
            imperfect = Some(VERDICT_LABELS[slot]);
        }
    }
    imperfect
}

/// Snapshot of the deep audit, aligned with [`VERDICT_LABELS`].
#[must_use]
pub fn deep_audit_tally() -> [u64; 8] {
    std::array::from_fn(|i| VERDICTS[i].load(Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

fn captured_shapes() -> &'static std::sync::Mutex<HashSet<(usize, u32)>> {
    static S: std::sync::OnceLock<std::sync::Mutex<HashSet<(usize, u32)>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(HashSet::new()))
}

/// Read the capture directory once. This sits on the transaction hot path, and
/// a per-transaction `env::var` is a syscall plus an allocation to answer a
/// question whose answer cannot change after startup.
fn capture_dir() -> Option<&'static Path> {
    static DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("CAPTURE_LOG_FIXTURES").map(Into::into))
        .as_deref()
}

/// Harvest one transaction as a fixture, if it is a *shape* we have not seen.
///
/// No-op unless `CAPTURE_LOG_FIXTURES` names a directory, so the firehose pays
/// one env lookup for this in normal operation.
///
/// Selection is deliberately narrow. A transaction with one program, or with no
/// CPI nesting, passes the invariant even when attribution is completely broken
/// — it cannot discriminate, so it is not evidence. Shapes are keyed by
/// (distinct programs, max depth) and kept once each: a hundred router swaps
/// teach nothing the first one did not, and an unbounded harvester fills a disk
/// with duplicates.
///
/// Writes are best-effort and never overwrite: a fixture already on disk is
/// evidence someone may have committed, and silently replacing it would let a
/// green suite drift off the data it was pinned to.
pub fn capture_fixture(
    signature: &str,
    slot: u64,
    instructions: &[ParsedInstruction],
    logs: &[String],
) {
    let Some(dir) = capture_dir() else {
        return;
    };
    let programs: HashSet<Pubkey> = instructions.iter().map(|i| i.program_id).collect();
    let depth = instructions
        .iter()
        .map(|i| i.stack_height)
        .max()
        .unwrap_or(0);
    if programs.len() < 2 || depth < 2 {
        return;
    }
    let key = (programs.len(), depth);
    {
        let Ok(mut seen) = captured_shapes().lock() else {
            return;
        };
        if !seen.insert(key) {
            return;
        }
    }

    write_fixture(
        dir,
        &format!("logs_{}programs_depth{depth}.json", key.0),
        signature,
        slot,
        instructions,
        logs,
    );
}

/// Keep a transaction whose attribution came out imperfect, one per verdict kind.
///
/// A counter says *how often* attribution fell short; only the transaction says
/// *why*. Without this the residual stays a number nobody can chase, which is how
/// a 0.01% tail lives forever.
pub fn capture_imperfect(
    verdict: &str,
    signature: &str,
    slot: u64,
    instructions: &[ParsedInstruction],
    logs: &[String],
) {
    let Some(dir) = capture_dir() else {
        return;
    };
    {
        let Ok(mut seen) = captured_verdicts().lock() else {
            return;
        };
        if !seen.insert(verdict.to_string()) {
            return;
        }
    }
    write_fixture(
        dir,
        &format!("imperfect_{verdict}.json"),
        signature,
        slot,
        instructions,
        logs,
    );
}

fn captured_verdicts() -> &'static std::sync::Mutex<HashSet<String>> {
    static S: std::sync::OnceLock<std::sync::Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(HashSet::new()))
}

fn write_fixture(
    dir: &Path,
    file: &str,
    signature: &str,
    slot: u64,
    instructions: &[ParsedInstruction],
    logs: &[String],
) {
    let fixture = LogFixture {
        signature: signature.to_string(),
        slot,
        instructions: instructions
            .iter()
            .map(|i| FixtureInstruction {
                program: i.program_id.to_string(),
                stack_height: i.stack_height,
            })
            .collect(),
        logs: logs.to_vec(),
    };
    let Ok(json) = serde_json::to_string_pretty(&fixture) else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = dir.join(file);
    if path.exists() {
        return;
    }
    let _ = std::fs::write(path, json);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(seed: u8) -> Pubkey {
        Pubkey::new_from_array([seed; 32])
    }

    fn ix(program: Pubkey, stack_height: u32, logs: Vec<LogEntry>) -> ParsedInstruction {
        let mut i = ParsedInstruction::new(program, vec![], vec![], stack_height, 0);
        i.logs = logs;
        i
    }

    #[test]
    fn well_bracketed_slice_passes() {
        let p = pk(1);
        let verdict = audit_log_slice(&ix(
            p,
            2,
            vec![
                LogEntry::Invoke {
                    program: p,
                    depth: 2,
                },
                LogEntry::Message {
                    message: "hi".into(),
                },
                LogEntry::Success { program: p },
            ],
        ));
        assert_eq!(verdict, LogSliceVerdict::Ok);
    }

    #[test]
    fn slice_opening_on_another_program_is_foreign() {
        let (mine, theirs) = (pk(1), pk(2));
        let verdict = audit_log_slice(&ix(
            mine,
            1,
            vec![
                LogEntry::Invoke {
                    program: theirs,
                    depth: 1,
                },
                LogEntry::Success { program: theirs },
            ],
        ));
        assert_eq!(verdict, LogSliceVerdict::ForeignOpen { found: theirs });
    }

    #[test]
    fn leftover_entry_from_previous_instruction_is_not_opened() {
        let p = pk(1);
        let verdict = audit_log_slice(&ix(p, 1, vec![LogEntry::Success { program: pk(2) }]));
        assert_eq!(verdict, LogSliceVerdict::NotOpened);
    }

    #[test]
    fn wrong_cpi_depth_is_reported_separately() {
        let p = pk(1);
        let verdict = audit_log_slice(&ix(
            p,
            3,
            vec![
                LogEntry::Invoke {
                    program: p,
                    depth: 2,
                },
                LogEntry::Success { program: p },
            ],
        ));
        assert_eq!(verdict, LogSliceVerdict::DepthMismatch { found: 2 });
    }

    #[test]
    fn a_childs_invoke_inside_the_slice_is_a_leak() {
        let (parent, child) = (pk(1), pk(2));
        let verdict = audit_log_slice(&ix(
            parent,
            1,
            vec![
                LogEntry::Invoke {
                    program: parent,
                    depth: 1,
                },
                LogEntry::Invoke {
                    program: child,
                    depth: 2,
                },
                LogEntry::Success { program: child },
                LogEntry::Success { program: parent },
            ],
        ));
        assert_eq!(verdict, LogSliceVerdict::InteriorInvoke);
    }

    #[test]
    fn slice_that_never_reaches_its_terminator() {
        let p = pk(1);
        let verdict = audit_log_slice(&ix(
            p,
            1,
            vec![
                LogEntry::Invoke {
                    program: p,
                    depth: 1,
                },
                LogEntry::Consumed {
                    program: p,
                    used: 10,
                    available: 100,
                },
            ],
        ));
        assert_eq!(verdict, LogSliceVerdict::Unterminated);
    }

    #[test]
    fn truncation_outranks_every_other_verdict() {
        // Deliberately also foreign-opening: truncation makes the rest
        // unfalsifiable, so it must be reported first.
        let verdict = audit_log_slice(&ix(
            pk(1),
            1,
            vec![
                LogEntry::Invoke {
                    program: pk(2),
                    depth: 9,
                },
                LogEntry::Truncated,
            ],
        ));
        assert_eq!(verdict, LogSliceVerdict::Truncated);
    }

    #[test]
    fn replay_reindexes_programs_and_preserves_order() {
        let (a, b) = (pk(1), pk(2));
        let fixture = LogFixture {
            signature: "sig".into(),
            slot: 1,
            instructions: vec![
                FixtureInstruction {
                    program: a.to_string(),
                    stack_height: 1,
                },
                FixtureInstruction {
                    program: b.to_string(),
                    stack_height: 2,
                },
                FixtureInstruction {
                    program: a.to_string(),
                    stack_height: 1,
                },
            ],
            logs: vec![
                format!("Program {a} invoke [1]"),
                format!("Program {b} invoke [2]"),
                format!("Program {b} success"),
                format!("Program {a} success"),
                format!("Program {a} invoke [1]"),
                format!("Program {a} success"),
            ],
        };
        let parsed = fixture.replay().expect("replay");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].program_id, a);
        assert_eq!(parsed[1].program_id, b);
        assert_eq!(parsed[2].program_id, a);
        assert_eq!(parsed[1].parent_index, Some(0));
    }

    #[test]
    fn replay_refuses_a_program_that_is_not_a_pubkey() {
        let fixture = LogFixture {
            signature: "sig".into(),
            slot: 1,
            instructions: vec![FixtureInstruction {
                program: "not-a-pubkey".into(),
                stack_height: 1,
            }],
            logs: vec![],
        };
        assert!(matches!(fixture.replay(), Err(FixtureError::BadProgram(_))));
    }
}
