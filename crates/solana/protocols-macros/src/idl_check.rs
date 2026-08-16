//! Compile-time account-layout verification against a vendored Anchor IDL.
//!
//! An instruction's account **order** is the one mechanical fact about it that
//! cannot be derived from anything we hold: a discriminator comes from the
//! name, a PDA from its seeds, but "account 12 is the pool's quote vault" is
//! data. So it has to be checked against the program's own IDL, and the check
//! has to be one nobody can skip.
//!
//! This runs at macro expansion. A struct whose fields disagree with the IDL —
//! wrong name, wrong order, wrong count — **fails to compile**, naming the
//! first slot that differs. There is no runtime cost and no test to forget to
//! run.
//!
//! # Why not `build.rs`, and why not fetch
//!
//! The existing `build.rs` hashes the IDL files. That detects a *changed IDL*;
//! it says nothing about whether our structs agree with it, which is the
//! actual question. Hash pinning and this check are complementary: one notices
//! the program moved, the other notices we did not follow.
//!
//! The IDL is **vendored**, never fetched during a build. A build that reaches
//! the network is a build that fails offline, fails in CI behind a proxy, and
//! fails when the RPC is down — which happened during the session that wrote
//! this. Refreshing the vendored IDL is a deliberate act with a diff to review,
//! which is exactly when a program upgrade should be noticed.
//!
//! # Prior art, and why this differs
//!
//! A common pattern is to compare hand-typed discriminators against an IDL *in
//! a test*. That keeps the hand-typed copy alive and the check skippable —
//! which is how a `[0u8; 8]` placeholder survived here for months. This does
//! not verify a copy; it makes the IDL the authority for the one fact we
//! cannot derive.

use std::path::PathBuf;

/// Locate a vendored IDL. Paths resolve against the *consuming* crate's
/// manifest dir, so the macro works from any working directory.
fn idl_path(program: &str) -> PathBuf {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(root)
        .join("idls")
        .join(format!("{program}.json"))
}

/// The account names an IDL declares for `instruction`, in order.
///
/// # Errors
///
/// Returns a human-readable reason when the IDL is absent, unparseable, or
/// does not declare the instruction — each of which must fail the build rather
/// than silently skip verification.
pub fn idl_accounts(program: &str, instruction: &str) -> Result<Vec<String>, String> {
    let path = idl_path(program);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let idl: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;

    let instructions = idl
        .get("instructions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{} has no `instructions` array", path.display()))?;

    let ix = instructions
        .iter()
        .find(|i| i.get("name").and_then(|n| n.as_str()) == Some(instruction))
        .ok_or_else(|| {
            let known: Vec<&str> = instructions
                .iter()
                .filter_map(|i| i.get("name").and_then(|n| n.as_str()))
                .collect();
            format!("{program}.json declares no instruction `{instruction}`; it has: {known:?}")
        })?;

    ix.get("accounts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("`{instruction}` has no `accounts` array"))?
        .iter()
        .map(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .map(str::to_owned)
                .ok_or_else(|| format!("`{instruction}` has an unnamed account"))
        })
        .collect()
}

/// Compare a struct's field names to the IDL's account names, in order.
///
/// Returns `Ok(())` when they agree, or a message naming the **first**
/// divergence — a whole-list dump is unreadable at 27 accounts, and the first
/// mismatch is where the layout actually forked.
///
/// # Errors
///
/// Any disagreement in count, order, or name.
pub fn check_accounts(program: &str, instruction: &str, fields: &[String]) -> Result<(), String> {
    let expected = idl_accounts(program, instruction)?;

    for (i, want) in expected.iter().enumerate() {
        match fields.get(i) {
            Some(got) if got == want => {}
            Some(got) => {
                return Err(format!(
                    "account {i} of `{instruction}` is `{want}` in {program}.json, \
                     but this struct declares `{got}`"
                ));
            }
            None => {
                return Err(format!(
                    "`{instruction}` has {} accounts in {program}.json; this struct \
                     declares only {} — first missing is {i} `{want}`",
                    expected.len(),
                    fields.len()
                ));
            }
        }
    }

    // A struct MAY stop short of the IDL's list: Anchor's trailing accounts are
    // optional and a decoder that reads the required prefix is correct. It may
    // never declare accounts the IDL does not have — that is a layout we
    // invented.
    if fields.len() > expected.len() {
        return Err(format!(
            "this struct declares {} accounts but `{instruction}` has {} in \
             {program}.json — `{}` is not in the IDL",
            fields.len(),
            expected.len(),
            fields[expected.len()]
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// The field names an IDL declares for an event type, in order.
///
/// # Errors
///
/// The IDL is absent, unparseable, or declares no such event type.
pub fn idl_event_fields(program: &str, event: &str) -> Result<Vec<String>, String> {
    let path = idl_path(program);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let idl: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;

    // Anchor keeps event *layouts* under `types`, and `events` only names them.
    let types = idl
        .get("types")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{} has no `types` array", path.display()))?;

    let ty = types
        .iter()
        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some(event))
        .ok_or_else(|| format!("{program}.json declares no type `{event}`"))?;

    ty.get("type")
        .and_then(|t| t.get("fields"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("`{event}` has no `fields` array"))?
        .iter()
        .map(|f| {
            f.get("name")
                .and_then(|n| n.as_str())
                .map(str::to_owned)
                .ok_or_else(|| format!("`{event}` has an unnamed field"))
        })
        .collect()
}

/// One of our event struct's fields, and whether the IDL is expected to know it.
pub struct EventField {
    /// The Rust field name.
    pub name: String,
    /// `Some(reason)` when the field is deliberately absent from the IDL.
    pub undeclared: Option<String>,
}

/// Compare an event struct's fields to the IDL's, in order.
///
/// Fields carrying `#[idl(undeclared = "...")]` are *skipped*: the program
/// emits bytes its published interface does not describe — measured on pump's
/// AMM 2026-08-15, 25 bytes past the last declared field on both trade events —
/// and a decoder that refuses to model them either loses the data or, because
/// borsh rejects trailing bytes, fails on every real body.
///
/// The exemption is per field and must state a reason, so "the IDL does not
/// declare this" is a claim someone wrote down rather than a silent gap. Every
/// other field must match the IDL by name and order.
///
/// Declared fields may stop short of the IDL's list — a decoder that reads a
/// prefix is correct — but may never rename or reorder one.
///
/// # Errors
///
/// Any disagreement in name or order among the non-exempt fields, or an
/// exemption reason that is empty.
pub fn check_event_fields(program: &str, event: &str, fields: &[EventField]) -> Result<(), String> {
    let expected = idl_event_fields(program, event)?;
    let mut want = expected.iter();

    for f in fields {
        if let Some(reason) = &f.undeclared {
            if reason.trim().is_empty() {
                return Err(format!(
                    "field `{}` of `{event}` is marked undeclared with no reason; \
                     say what it is, or `unknown` if that is the honest answer",
                    f.name
                ));
            }
            continue;
        }
        match want.next() {
            Some(w) if *w == f.name => {}
            Some(w) => {
                return Err(format!(
                    "`{event}` in {program}.json declares `{w}` where this struct \
                     has `{}` — if the program emits it but the IDL does not, mark \
                     it `#[idl(undeclared = \"...\")]`",
                    f.name
                ));
            }
            None => {
                return Err(format!(
                    "`{event}` has {} fields in {program}.json; this struct declares \
                     `{}` past the end — mark it `#[idl(undeclared = \"...\")]` if the \
                     program emits it anyway",
                    expected.len(),
                    f.name
                ));
            }
        }
    }
    Ok(())
}
