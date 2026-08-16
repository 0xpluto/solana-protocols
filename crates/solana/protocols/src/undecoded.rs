//! Instructions we failed to decode — kept, counted, and reported.
//!
//! An extractor that cannot parse an instruction returns `None`, which is
//! indistinguishable from "this is not an event I model". That conflation is
//! not hypothetical: five pumpfun and pumpswap swap instructions went
//! undecoded for months and accounted for **19.7% of all swaps**, and nothing
//! counted them, because no row and no error is exactly what "not a swap"
//! looks like.
//!
//! The three sites that detected the failure logged it at `trace!` — the
//! quietest level available — so the loudest evidence in the system was
//! emitted where nobody would ever enable it. Logging is also not enough on
//! its own: during the session that wrote this, a `RUST_LOG` filter scoped to
//! one target hid these very warnings and nearly produced a report that they
//! had stopped occurring.
//!
//! So three things happen, deliberately, rather than one:
//!
//! 1. **Counted** — a process-wide tally per `(program, discriminator)`, which
//!    survives log filtering and answers "is this still happening".
//! 2. **Kept** — the raw data and accounts are captured, because the whole
//!    point is to fix the parser later and that needs the bytes, not a note
//!    that bytes existed.
//! 3. **Reported** — at `warn!`, not `trace!`.
//!
//! The capture is bounded. An unparseable instruction that arrives at firehose
//! rate would otherwise consume memory in proportion to the failure, and a
//! failure whose logging is proportional to its rate destroys its own evidence
//! — 22h of venue rejections once consumed an entire 4GB journal here. We keep
//! the FIRST sample per discriminator, which is what a parser author needs, and
//! count the rest.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use solana_program::pubkey::Pubkey;
use tracing::warn;

/// A retained example of an instruction we could not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndecodedSample {
    /// Owning program.
    pub program: Pubkey,
    /// First 8 bytes, or fewer if the instruction carried fewer.
    pub discriminator: Vec<u8>,
    /// The complete instruction data — what a parser author needs.
    pub data: Vec<u8>,
    /// The instruction's accounts, in order.
    pub accounts: Vec<Pubkey>,
    /// How many times this `(program, discriminator)` has been seen.
    pub seen: u64,
}

static TOTAL: AtomicU64 = AtomicU64::new(0);

/// Keyed by `(program, discriminator)` — the identity of an instruction shape.
type ShapeKey = (Pubkey, Vec<u8>);
type SampleMap = Mutex<HashMap<ShapeKey, UndecodedSample>>;

fn samples() -> &'static SampleMap {
    static S: std::sync::OnceLock<SampleMap> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record an instruction the owning program's parser rejected.
///
/// Call this only for programs we *claim* to decode. An unknown program is not
/// a failure — it is the rest of Solana — and recording it would bury the
/// signal this exists to surface.
pub fn report(program: &Pubkey, data: &[u8], accounts: &[Pubkey], reason: &str) {
    TOTAL.fetch_add(1, Ordering::Relaxed);
    let disc = data.iter().take(8).copied().collect::<Vec<u8>>();
    let key = (*program, disc.clone());

    let Ok(mut map) = samples().lock() else {
        return; // a poisoned mutex must not take down the firehose
    };
    match map.get_mut(&key) {
        Some(existing) => {
            existing.seen += 1;
        }
        None => {
            // First sighting: loud, with everything needed to write the parser.
            warn!(
                program = %program,
                discriminator = ?disc,
                data_len = data.len(),
                n_accounts = accounts.len(),
                reason,
                "undecoded instruction on a program we claim to decode — \
                 sample retained, use `undecoded::report_all()` to recover it"
            );
            map.insert(
                key,
                UndecodedSample {
                    program: *program,
                    discriminator: disc,
                    data: data.to_vec(),
                    accounts: accounts.to_vec(),
                    seen: 1,
                },
            );
        }
    }
}

/// Every retained sample, with its running count. The tally is the answer to
/// "what are we still missing", and each sample is the input to fixing it.
#[must_use]
pub fn report_all() -> Vec<UndecodedSample> {
    samples()
        .lock()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default()
}

/// Total undecoded instructions seen, including repeats of a known shape.
#[must_use]
pub fn total() -> u64 {
    TOTAL.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sample_is_kept_and_repeats_are_counted() {
        let program = Pubkey::new_unique();
        let a = vec![Pubkey::new_unique()];
        report(&program, &[9, 9, 9, 9, 9, 9, 9, 9, 42], &a, "test");
        report(&program, &[9, 9, 9, 9, 9, 9, 9, 9, 43], &a, "test");

        let mine: Vec<_> = report_all()
            .into_iter()
            .filter(|s| s.program == program)
            .collect();
        assert_eq!(mine.len(), 1, "one sample per (program, discriminator)");
        assert_eq!(mine[0].seen, 2, "repeats counted, not re-stored");
        // The FIRST body is what was kept — a later one is not more useful and
        // overwriting would make the retained bytes drift under a hot failure.
        assert_eq!(mine[0].data.last(), Some(&42));
        assert!(total() >= 2);
    }

    #[test]
    fn different_discriminators_are_separate_samples() {
        let program = Pubkey::new_unique();
        report(&program, &[1; 8], &[], "a");
        report(&program, &[2; 8], &[], "b");
        assert_eq!(
            report_all().iter().filter(|s| s.program == program).count(),
            2
        );
    }

    /// An instruction shorter than a discriminator still gets kept — a
    /// truncated instruction is exactly the kind of thing worth seeing.
    #[test]
    fn a_short_instruction_is_still_retained() {
        let program = Pubkey::new_unique();
        report(&program, &[7], &[], "short");
        let s = report_all()
            .into_iter()
            .find(|s| s.program == program)
            .expect("retained");
        assert_eq!(s.discriminator, vec![7]);
    }
}
