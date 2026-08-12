//! Build script for solana-protocols.
//!
//! When the `idl-verify` feature is enabled, this verifies that the IDL files
//! in `idls/` match their expected hashes. If any hash mismatches, the build fails.

#[cfg(feature = "idl-verify")]
use sha2::{Digest, Sha256};
#[cfg(feature = "idl-verify")]
use std::path::Path;

/// Expected IDL hashes. Update these when protocol IDLs change.
///
/// To get a hash: `sha256sum idls/pumpfun.json | cut -d' ' -f1`
///
/// Empty until real IDLs are fetched into `idls/`. A placeholder entry here
/// would be a hash that can never match anything, sitting in a table that reads
/// as configured — the feature must do nothing rather than pretend.
#[cfg(feature = "idl-verify")]
const EXPECTED_IDL_HASHES: &[(&str, &str)] = &[];

#[cfg(feature = "idl-verify")]
fn sha256_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

fn main() {
    // Re-run if IDL files change
    println!("cargo::rerun-if-changed=idls/");

    // Only verify when feature is enabled
    #[cfg(feature = "idl-verify")]
    verify_idls();
}

#[cfg(feature = "idl-verify")]
fn verify_idls() {
    let idl_dir = Path::new("idls");

    let mut failures = Vec::new();
    let mut missing = Vec::new();

    for (protocol, expected_hash) in EXPECTED_IDL_HASHES {
        let idl_path = idl_dir.join(format!("{}.json", protocol));

        if !idl_path.exists() {
            missing.push(*protocol);
            continue;
        }

        let content = match std::fs::read(&idl_path) {
            Ok(c) => c,
            Err(e) => {
                println!(
                    "cargo::warning=Failed to read {}: {}",
                    idl_path.display(),
                    e
                );
                continue;
            }
        };

        let actual_hash = sha256_hash(&content);

        if actual_hash != *expected_hash {
            failures.push((*protocol, expected_hash.to_string(), actual_hash));
        } else {
            println!("cargo::warning=IDL verified: {} ✓", protocol);
        }
    }

    // A missing IDL is a FAILURE, not a warning. The caller asked for
    // verification; reporting success for a check that never ran is
    // indistinguishable from the check passing.
    if !missing.is_empty() {
        for protocol in &missing {
            println!(
                "cargo::error=IDL missing: idls/{}.json - fetch with: solana idl fetch <PROGRAM_ID>",
                protocol
            );
        }
        panic!(
            "IDL verification requested but {} IDL file(s) are absent. See errors above.",
            missing.len()
        );
    }

    // Report mismatches as errors
    if !failures.is_empty() {
        for (protocol, expected, actual) in &failures {
            println!("cargo::error=IDL hash mismatch for {}:", protocol);
            println!("cargo::error=  expected: {}", expected);
            println!("cargo::error=  actual:   {}", actual);
            println!(
                "cargo::error=  Update EXPECTED_IDL_HASHES in build.rs if this is intentional"
            );
        }
        panic!(
            "IDL verification failed for {} protocol(s). See errors above.",
            failures.len()
        );
    }
}
