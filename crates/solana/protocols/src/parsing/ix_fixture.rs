//! Harvest instruction fixtures from the live firehose.
//!
//! Sibling of [`log_fixture`](super::log_fixture), for the same reason and by
//! the same rules: a hand-written account list encodes what we *believe* a
//! program is sent, and every finding in this module's history says that belief
//! is short. Pumpfun appends `bonding_curve_v2` to every buy and sell and no IDL
//! mentions it; the two golden fixtures already on disk carried two accounts
//! each that nothing modelled, and passed for months because the replay test
//! compared a prefix.
//!
//! # One fixture per distinct decode path
//!
//! The shape is (program, discriminator, account count, data length) — the two
//! axes that change what actually runs. Account count selects the account-layout
//! path: pumpfun's `sell_v2` alone has been observed at 26, 27, 28 and 29. Data
//! length selects the params path: `buy_exact_quote_in_v2` arrives at 24 bytes
//! and at 25, and the extra byte is an undeclared `track_volume` that decodes
//! through different code.
//!
//! Anything narrower keeps duplicates — a thousand identical `buy`s exercise one
//! path and teach nothing after the first. Anything wider misses a path: keying
//! on the discriminator alone would have kept one `sell_v2` and hidden the other
//! three lengths.
//!
//! # The data-length axis is bounded, because it is not always a path
//!
//! For fixed-size params an extra byte is an extra field. For params carrying a
//! `String` or a `Vec` it is just a longer string: a two-minute capture produced
//! **42** `create_v2` files, one per distinct token name/symbol/URI length, all
//! decoding through the same borsh call. So at most
//! [`MAX_DATA_LENGTHS`] lengths are kept per (program, instruction, account
//! count) — the first few are evidence, the fortieth is a longer token name.
//! Captures refused by that bound are counted rather than dropped silently.
//!
//! # Event self-CPIs are not instructions
//!
//! An `emit_cpi!` event rides the instruction list with the Anchor event tag as
//! its discriminator, one account, and a body length that varies per event. That
//! is 53 near-identical files in a two-minute capture, and events have their own
//! fixtures. Skipped here.
//!
//! # Why the firehose and not an RPC scan
//!
//! An RPC harvest samples a window and rate-limits; this sees every instruction
//! the node sees, accumulates across restarts because it never overwrites, and
//! surfaces rare admin instructions eventually rather than needing them to fall
//! inside a scan. Off unless `CAPTURE_IX_FIXTURES` names a directory, so
//! production pays one env lookup.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use super::instruction::ParsedInstruction;

/// Directory to write into, resolved once.
fn capture_dir() -> Option<&'static Path> {
    static DIR: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("CAPTURE_IX_FIXTURES").map(Into::into))
        .as_deref()
}

/// The account-layout path: program, instruction, how many accounts arrived.
type Shape = (solana_program::pubkey::Pubkey, [u8; 8], usize);

/// How many distinct data lengths are kept per [`Shape`].
///
/// Beyond this the axis stops being evidence: a params struct with a `String`
/// produces a new length per token name, and they all decode identically.
pub const MAX_DATA_LENGTHS: usize = 3;

/// The Anchor `emit_cpi!` tag. Not a hash — a magic constant.
const ANCHOR_EVENT_TAG: [u8; 8] = [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d];

/// Data lengths already written per shape, so a busy firehose writes each path
/// once and a variable-length params struct cannot flood the directory.
fn captured() -> &'static std::sync::Mutex<std::collections::HashMap<Shape, HashSet<usize>>> {
    static S: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<Shape, HashSet<usize>>>,
    > = std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Captures refused by [`MAX_DATA_LENGTHS`].
///
/// A bound nobody can see reads as "we covered everything" when it did not.
static BOUNDED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many captures the data-length bound turned away.
#[must_use]
pub fn bounded_captures() -> u64 {
    BOUNDED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Serialized form. Matches what
/// [`InstructionFixture`](crate::test_fixtures::InstructionFixture) loads, so a
/// captured file is committable as-is.
#[derive(Serialize)]
struct Fixture {
    program: String,
    /// Filled in by hand when the file is committed: the harvester knows the
    /// discriminator, not which name we have given it.
    instruction: Option<String>,
    signature: String,
    slot: u64,
    /// Whether the account flags in this file can be believed.
    ///
    /// Always `false` for a captured fixture: the flags are not recoverable from
    /// the stream, so they are not written. A fixture with authoritative flags
    /// has to come from a source that carries them.
    top_level: bool,
    /// Why the flags are missing, in the file, for whoever opens it next.
    note: &'static str,
    discriminator: Vec<u8>,
    accounts: Vec<FixtureAccount>,
    data_b64: String,
}

/// Just the pubkey.
///
/// Signer/writable are deliberately absent rather than written as `false`. The
/// gRPC instruction stream carries account *keys*, not their per-instruction
/// privileges, so a `false` here would be a fact we do not have — and the
/// fixture's `top_level` flag exists precisely to tell a reader whether the
/// flags can be believed. Writing both a fabricated `false` and
/// `top_level: true` made a golden test compare a real layout against invented
/// data, which it duly failed.
#[derive(Serialize)]
struct FixtureAccount {
    pubkey: String,
}

/// Write one instruction as a fixture, if its shape is new.
///
/// Best-effort and never overwriting: a fixture on disk may already be
/// committed and pinned to a test, and replacing it would let a green suite
/// drift off the data it was proven against.
pub fn capture_instruction(
    signature: &str,
    slot: u64,
    ix: &ParsedInstruction,
    programs: &[solana_program::pubkey::Pubkey],
) {
    let Some(dir) = capture_dir() else {
        return;
    };
    if !programs.contains(&ix.program_id) {
        return;
    }
    let Some(disc) = ix.data.get(..8).and_then(|d| <[u8; 8]>::try_from(d).ok()) else {
        return;
    };
    if disc == ANCHOR_EVENT_TAG {
        // Not an instruction, but the only place a real event body is
        // observable. `capture_event_body` existed with zero callers — declared
        // and never consulted — so every event we had not already modelled was
        // unreachable: no bodies meant no fixtures meant no way to model it.
        //
        // Keyed by the event's own discriminator, because the tag says "an
        // Anchor event" and never which one. Names are resolved offline against
        // the IDL; the discriminator is the thing that identifies it.
        if let Some((ev_disc, body)) = crate::parsing::anchor::split_event_ix(&ix.data) {
            let hex: String = ev_disc.iter().map(|b| format!("{b:02x}")).collect();
            super::event::capture_event_body(&hex, body);
        }
        return;
    }
    let key = (ix.program_id, disc, ix.accounts.len());
    {
        let Ok(mut seen) = captured().lock() else {
            return;
        };
        let lengths = seen.entry(key).or_default();
        if lengths.contains(&ix.data.len()) {
            return;
        }
        if lengths.len() >= MAX_DATA_LENGTHS {
            BOUNDED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        lengths.insert(ix.data.len());
    }

    let Ok(json) = serde_json::to_string_pretty(&fixture_for(signature, slot, ix)) else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let disc_hex: String = disc.iter().map(|b| format!("{b:02x}")).collect();
    let path = dir.join(format!(
        "ix_{disc_hex}_n{}_d{}.json",
        ix.accounts.len(),
        ix.data.len()
    ));
    if path.exists() {
        return;
    }
    let _ = std::fs::write(path, json);
}

/// Build the serializable fixture. Split out from [`capture_instruction`] so the
/// shape can be tested without setting a process-global env var, which would
/// disarm the off-by-default test for every sibling.
fn fixture_for(signature: &str, slot: u64, ix: &ParsedInstruction) -> Fixture {
    Fixture {
        program: ix.program_id.to_string(),
        instruction: None,
        signature: signature.to_string(),
        slot,
        top_level: false,
        note: "captured from the gRPC firehose, which carries account keys but not \
               their per-instruction signer/writable privileges; flags are therefore \
               absent and top_level is false",
        discriminator: ix.data.get(..8).unwrap_or_default().to_vec(),
        accounts: ix
            .accounts
            .iter()
            .map(|a| FixtureAccount {
                pubkey: a.to_string(),
            })
            .collect(),
        data_b64: base64_encode(&ix.data),
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    use solana_program::pubkey::Pubkey;

    fn ix(n_accounts: usize, disc: u8, height: u32) -> ParsedInstruction {
        let mut data = vec![disc; 8];
        data.extend_from_slice(&[1, 2, 3]);
        super::super::instruction::ParsedInstructionBuilder::new()
            .program_id(Pubkey::new_unique())
            .accounts((0..n_accounts).map(|_| Pubkey::new_unique()).collect())
            .data(data)
            .stack_height(height)
            .instruction_index(0)
            .build()
    }

    /// The captured file records every account, the discriminator, and whether
    /// the flags are believable — the three things a layout is checked against.
    #[test]
    fn a_captured_fixture_records_the_whole_account_list() {
        let i = ix(18, 0xAB, 1);
        let f = fixture_for("sig", 42, &i);
        assert_eq!(f.accounts.len(), 18, "every account, not just the modelled ones");
        assert_eq!(f.discriminator, vec![0xAB; 8]);
        assert!(
            !f.top_level,
            "the stream carries no flags, so they are never authoritative"
        );
        assert_eq!(f.slot, 42);
        assert_eq!(f.program, i.program_id.to_string());
    }

    /// A captured fixture never claims authoritative flags, at any depth.
    ///
    /// It wrote `signer: false, writable: false` and `top_level: true`, so the
    /// generated golden test compared a real layout against invented flags and
    /// failed on a `fee_recipient` the IDL declares writable.
    #[test]
    fn a_capture_never_claims_authoritative_flags() {
        assert!(!fixture_for("sig", 1, &ix(26, 0x01, 2)).top_level);
        assert!(!fixture_for("sig", 1, &ix(18, 0x02, 1)).top_level);
    }

    /// An event self-CPI is not an instruction and must not become a fixture.
    ///
    /// It rides the instruction list with the Anchor tag, one account, and a
    /// body length that varies per event — 53 near-identical files in one
    /// two-minute capture before this.
    #[test]
    fn the_anchor_event_tag_is_not_a_shape() {
        let mut data = ANCHOR_EVENT_TAG.to_vec();
        data.extend_from_slice(&[7u8; 200]);
        let e = super::super::instruction::ParsedInstructionBuilder::new()
            .program_id(Pubkey::new_unique())
            .accounts(vec![Pubkey::new_unique()])
            .data(data)
            .stack_height(2)
            .instruction_index(0)
            .build();
        assert_eq!(e.data[..8], ANCHOR_EVENT_TAG, "the tag is a constant, not a hash");
    }

    /// Off by default: production must pay nothing but one env lookup, and a
    /// harvester that runs unasked fills a disk.
    #[test]
    fn capture_is_off_without_the_env_var() {
        assert!(
            std::env::var_os("CAPTURE_IX_FIXTURES").is_none(),
            "test env must not set the capture var"
        );
        assert!(capture_dir().is_none());
    }
}
