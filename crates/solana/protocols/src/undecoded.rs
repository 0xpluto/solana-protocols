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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
static CAPTURE: AtomicBool = AtomicBool::new(false);

/// Turn body retention on. Off by default: a decoder library should not hold
/// samples of every unknown instruction it sees unless someone asked it to.
pub fn enable_capture() {
    CAPTURE.store(true, Ordering::Relaxed);
}

/// Whether bodies are being retained.
#[must_use]
pub fn capture_enabled() -> bool {
    CAPTURE.load(Ordering::Relaxed)
}

/// Keyed by `(program, discriminator, data_len, account_count)`.
///
/// The discriminator alone is not the shape. One instruction legitimately
/// appears in several forms — an optional trailing argument, optional trailing
/// accounts — and those variants are the interesting part: they are what a
/// decoder has to handle and what an incomplete IDL hides. Keying on the
/// discriminator alone keeps the first form seen and discards every other,
/// which is how a 24-byte and a 25-byte `buy_exact_quote_in_v2` would have
/// looked like one thing.
///
/// Widening the key keeps each distinct form once, and only once: repeats of a
/// form still increment a counter rather than accumulating samples, so a
/// firehose run yields a handful of shapes, not thousands of near-identical
/// bodies.
type ShapeKey = (Pubkey, Vec<u8>, usize, usize);
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
    // The tally is always cheap and always on — "how much are we missing" must
    // not depend on a flag. Retaining bodies is what costs memory, so that is
    // the part behind the switch.
    TOTAL.fetch_add(1, Ordering::Relaxed);
    if !capture_enabled() {
        return;
    }
    let disc = data.iter().take(8).copied().collect::<Vec<u8>>();
    let key = (*program, disc.clone(), data.len(), accounts.len());

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

/// `CAPTURE` is process-global, so a test that turns it off runs concurrently
/// with — and silently disarms — every test that needs it on. Samples are
/// already isolated (each test uses a fresh program id); the flag is not.
///
/// The guard recovers from poisoning: one panicking test must fail alone, not
/// take the rest of the module with it.
#[cfg(test)]
pub(crate) fn capture_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sample_is_kept_and_repeats_are_counted() {
        let _serialized = capture_lock();
        enable_capture();
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

    /// The same instruction in two shapes is two samples, because the variants
    /// are the point: an optional trailing argument or account is exactly what
    /// a decoder must handle and what an incomplete IDL hides.
    #[test]
    fn one_instruction_in_two_shapes_is_two_samples() {
        let _serialized = capture_lock();
        enable_capture();
        let program = Pubkey::new_unique();
        let disc = [4u8; 8];
        let one = vec![Pubkey::new_unique()];
        let two = vec![Pubkey::new_unique(), Pubkey::new_unique()];

        report(&program, &disc, &one, "a"); // 8 bytes, 1 account
        report(&program, &disc, &one, "a"); // repeat: counted, not stored
        let mut longer = disc.to_vec();
        longer.push(1);
        report(&program, &longer, &one, "b"); // 9 bytes, 1 account
        report(&program, &disc, &two, "c"); // 8 bytes, 2 accounts

        let mine: Vec<_> = report_all()
            .into_iter()
            .filter(|s| s.program == program)
            .collect();
        assert_eq!(mine.len(), 3, "three distinct shapes, one entry each");
        assert_eq!(
            mine.iter().map(|s| s.seen).sum::<u64>(),
            4,
            "the repeat is counted, not discarded"
        );
    }

    /// With capture off the tally still moves — "how much are we missing" must
    /// not depend on a flag — but no bodies are held.
    #[test]
    fn the_tally_works_with_capture_disabled() {
        let _serialized = capture_lock();
        CAPTURE.store(false, Ordering::Relaxed);
        let program = Pubkey::new_unique();
        let before = total();
        report(&program, &[8u8; 8], &[], "off");
        assert!(total() > before, "tally is unconditional");
        assert!(
            !report_all().iter().any(|s| s.program == program),
            "no body retained while capture is off"
        );
        // Restore the flag for everyone else; the guard above is still held,
        // so no concurrent test observed it off.
        enable_capture();
    }

    #[test]
    fn different_discriminators_are_separate_samples() {
        let _serialized = capture_lock();
        enable_capture();
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
        let _serialized = capture_lock();
        enable_capture();
        let program = Pubkey::new_unique();
        report(&program, &[7], &[], "short");
        let s = report_all()
            .into_iter()
            .find(|s| s.program == program)
            .expect("retained");
        assert_eq!(s.discriminator, vec![7]);
    }
}

/// Write every retained sample to `dir` as a fixture JSON, one file per
/// `(program, discriminator)`.
///
/// This is the point of retaining them. A counter says how much we are
/// missing; a fixture is what lets someone write the decoder and prove it
/// against the bytes the chain actually produced — the same standard
/// `OnchainAccount` and `OnchainInstruction` already enforce, reached from the
/// firehose instead of a hand-run capture script.
///
/// Existing files are not overwritten: the first capture of a shape is the one
/// a parser was written against, and silently replacing it would move the
/// goalposts under a passing test.
///
/// # Errors
///
/// The directory cannot be created or a fixture cannot be written.
pub fn dump_fixtures(dir: &std::path::Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let mut written = 0;
    for s in report_all() {
        let disc: String = s.discriminator.iter().map(|b| format!("{b:02x}")).collect();
        let path = dir.join(format!("{}_{}.json", &s.program.to_string()[..8], disc));
        if path.exists() {
            continue;
        }
        let json = format!(
            "{{\n  \"program\": \"{}\",\n  \"discriminator\": {:?},\n  \"seen\": {},\n  \
             \"captured_at\": \"firehose\",\n  \"data_len\": {},\n  \"data_b64\": \"{}\",\n  \
             \"accounts\": [{}]\n}}\n",
            s.program,
            s.discriminator,
            s.seen,
            s.data.len(),
            base64_encode(&s.data),
            s.accounts
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
        std::fs::write(&path, json)?;
        written += 1;
    }
    Ok(written)
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod dump_tests {
    use super::*;

    #[test]
    fn samples_land_as_fixtures_and_are_not_overwritten() {
        let _serialized = capture_lock();
        enable_capture();
        let dir = std::env::temp_dir().join(format!("undec_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let program = Pubkey::new_unique();
        report(
            &program,
            &[3, 1, 4, 1, 5, 9, 2, 6, 0xAA],
            &[Pubkey::new_unique()],
            "t",
        );

        let n = dump_fixtures(&dir).expect("dump");
        assert!(n >= 1);
        let before = std::fs::read_dir(&dir).unwrap().count();

        // A second dump must not rewrite what a parser may already be pinned to.
        let again = dump_fixtures(&dir).expect("dump again");
        assert_eq!(again, 0, "existing fixtures must not be overwritten");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
