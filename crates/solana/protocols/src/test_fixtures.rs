//! Golden on-chain fixtures — the layout-truth half of decoder verification.
//!
//! A fixture freezes a real mainnet account's bytes plus the field values it must
//! decode to. The test decodes the frozen bytes and asserts the declared
//! `expected` fields match. This verifies layout against **the chain** (ground
//! truth), needs no IDL, and works identically for Anchor and non-Anchor accounts
//! (see `docs/solana-decoder-verification-spec.md`). Test-only.
//!
//! Fixtures live at `crates/solana/protocols/fixtures/<program>/<name>.json` and
//! are captured with the `gen_fixtures` helper (raw base64 + decoded snapshot).

use serde::Serialize;

/// A loaded account fixture: the frozen bytes and the field values they must
/// produce.
pub struct AccountFixture {
    /// The account address the bytes were captured from (for assertion messages).
    pub address: String,
    /// Slot the bytes were captured at.
    pub slot: u64,
    data: Vec<u8>,
    /// The subset of decoded fields this fixture pins (serde field name → value).
    expected: serde_json::Map<String, serde_json::Value>,
}

impl AccountFixture {
    /// Load a fixture by its path relative to the crate's `fixtures/` dir, e.g.
    /// `"pumpswap/pool_v3_full_301.json"`. Panics with a clear message on any
    /// malformed fixture — a broken fixture is a broken test, not a silent skip.
    #[must_use]
    pub fn load(rel: &str) -> Self {
        let path = format!("{}/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"));
        let v: serde_json::Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("fixture {path}: bad json: {e}"));
        let b64 = v["data_b64"]
            .as_str()
            .unwrap_or_else(|| panic!("fixture {path}: missing data_b64"));
        let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .unwrap_or_else(|e| panic!("fixture {path}: bad base64: {e}"));
        let expected = v["expected"]
            .as_object()
            .cloned()
            .unwrap_or_else(|| panic!("fixture {path}: missing expected object"));
        Self {
            address: v["address"].as_str().unwrap_or_default().to_string(),
            slot: v["slot"].as_u64().unwrap_or_default(),
            data,
            expected,
        }
    }

    /// The frozen raw account bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// A pinned `expected` field as a string (for decoders that don't derive
    /// `Serialize`, so the test cross-checks individual fields by hand). Panics
    /// if the field is missing or not a string.
    #[must_use]
    pub fn expected_str(&self, field: &str) -> &str {
        self.expected
            .get(field)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("fixture {}: expected string field `{field}`", self.address))
    }

    /// A pinned `expected` field as a signed integer. Panics if missing/not an int.
    #[must_use]
    pub fn expected_i64(&self, field: &str) -> i64 {
        self.expected
            .get(field)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_else(|| panic!("fixture {}: expected int field `{field}`", self.address))
    }

    /// Assert every field the fixture pins matches the decoded value. `decoded`
    /// is any `Serialize` type (the decoder's output); comparison is by serde
    /// field name, so a fixture may pin a subset of fields. Fails loudly naming
    /// the field, the account, and both values.
    pub fn assert_matches<S: Serialize>(&self, decoded: &S) {
        let got = serde_json::to_value(decoded).expect("decoded value serializes");
        for (field, want) in &self.expected {
            let have = got.get(field).map(normalize);
            assert_eq!(
                have.as_ref(),
                Some(want),
                "fixture {} ({}B @ slot {}): field `{field}` = {have:?}, expected {want}",
                self.address,
                self.data.len(),
                self.slot,
            );
        }
    }
}

/// A loaded **instruction** fixture: a real landed instruction's ordered
/// accounts (pubkey + signer/writable flags, ALT-resolved) and data.
///
/// The `OnchainInstruction` derive asserts `from_pubkeys(pubkeys)` reproduces
/// the account order (pubkeys), and — only for `top_level` fixtures — the
/// signer/writable flags too. Flags are authoritative only at the message top
/// level; an inner (CPI) instruction's declared per-account privileges aren't
/// recoverable from tx data, so flag-checking is skipped there. The
/// [`account_metas`](Self::account_metas) helper (pubkey + flags) is the ground
/// truth a *built* instruction is verified against.
pub struct InstructionFixture {
    /// The transaction the instruction was captured from.
    pub signature: String,
    /// Slot the instruction landed at.
    pub slot: u64,
    /// True iff captured as a top-level (not inner/CPI) instruction, i.e. its
    /// signer/writable flags are authoritative.
    top_level: bool,
    accounts: Vec<(solana_program::pubkey::Pubkey, bool, bool)>,
    data: Vec<u8>,
}

impl InstructionFixture {
    /// Load an instruction fixture by its path relative to `fixtures/`, e.g.
    /// `"pumpswap/ix_buy.json"`. Panics with a clear message on any malformed
    /// fixture.
    #[must_use]
    pub fn load(rel: &str) -> Self {
        use std::str::FromStr;
        let path = format!("{}/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"));
        let v: serde_json::Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("fixture {path}: bad json: {e}"));
        let accounts = v["accounts"]
            .as_array()
            .unwrap_or_else(|| panic!("fixture {path}: missing accounts"))
            .iter()
            .map(|a| {
                let pk = solana_program::pubkey::Pubkey::from_str(a["pubkey"].as_str().unwrap())
                    .unwrap_or_else(|e| panic!("fixture {path}: bad pubkey: {e}"));
                (
                    pk,
                    a["signer"].as_bool().unwrap_or(false),
                    a["writable"].as_bool().unwrap_or(false),
                )
            })
            .collect();
        let data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            v["data_b64"]
                .as_str()
                .unwrap_or_else(|| panic!("fixture {path}: missing data_b64")),
        )
        .unwrap_or_else(|e| panic!("fixture {path}: bad base64: {e}"));
        Self {
            signature: v["signature"].as_str().unwrap_or_default().to_string(),
            slot: v["slot"].as_u64().unwrap_or_default(),
            top_level: v["top_level"].as_bool().unwrap_or(false),
            accounts,
            data,
        }
    }

    /// The instruction's ordered account pubkeys — feed to `from_pubkeys`.
    #[must_use]
    pub fn pubkeys(&self) -> Vec<solana_program::pubkey::Pubkey> {
        self.accounts.iter().map(|(pk, _, _)| *pk).collect()
    }

    /// Whether the flags are authoritative (captured at the message top level).
    #[must_use]
    pub fn top_level(&self) -> bool {
        self.top_level
    }

    /// The instruction's ordered account metas with the **real** signer/writable
    /// flags. Ground truth for verifying a *built* instruction's metas (flags
    /// included) — only meaningful when [`top_level`](Self::top_level).
    #[must_use]
    pub fn account_metas(&self) -> Vec<solana_sdk::instruction::AccountMeta> {
        self.accounts
            .iter()
            .map(|(pk, signer, writable)| {
                if *writable {
                    solana_sdk::instruction::AccountMeta::new(*pk, *signer)
                } else {
                    solana_sdk::instruction::AccountMeta::new_readonly(*pk, *signer)
                }
            })
            .collect()
    }

    /// The raw instruction data (discriminator prefix included).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// Render a 32-byte JSON array as its base58 string so fixtures can pin pubkeys
/// in readable form regardless of whether the decoder's `Pubkey` serializes as
/// bytes or as a string. Non-pubkey values (numbers, bools, strings) pass through.
fn normalize(v: &serde_json::Value) -> serde_json::Value {
    if let Some(arr) = v.as_array() {
        if arr.len() == 32 {
            let bytes: Option<Vec<u8>> = arr
                .iter()
                .map(|x| x.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect();
            if let Some(bytes) = bytes {
                if let Ok(pk) = solana_program::pubkey::Pubkey::try_from(bytes.as_slice()) {
                    return serde_json::Value::String(pk.to_string());
                }
            }
        }
    }
    v.clone()
}
