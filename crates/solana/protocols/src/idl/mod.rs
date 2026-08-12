//! IDL verification infrastructure.
//!
//! This module provides build-time verification that our instruction layouts
//! match the on-chain program IDLs. When a protocol updates their program,
//! the IDL hash will change and our build will fail, alerting us to update
//! our implementation.
//!
//! # Usage
//!
//! 1. Store IDL JSON files in `idls/` directory
//! 2. Register expected hashes in [`EXPECTED_IDL_HASHES`]
//! 3. Run verification with `verify_all_idls()`
//!
//! # Adding a New Protocol
//!
//! 1. Fetch IDL: `solana idl fetch <PROGRAM_ID> -o idls/protocol.json`
//! 2. Calculate hash: `sha256sum idls/protocol.json`
//! 3. Add entry to [`EXPECTED_IDL_HASHES`]

use crate::Protocol;

#[cfg(feature = "idl-verify")]
use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Expected IDL hashes for each protocol.
///
/// These are SHA-256 hashes of the IDL JSON files.
/// When a protocol updates their program, fetch new IDL and update hash.
///
/// To get a hash: `sha256sum idls/pumpfun.json | cut -d' ' -f1`
pub static EXPECTED_IDL_HASHES: &[(&str, &str)] = &[
    // Pumpfun IDL hash (update when program changes)
    (
        "pumpfun",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", // placeholder
    ),
    // Add more protocols here as they're implemented
];

/// IDL verification result.
#[derive(Debug, Clone)]
pub struct IdlVerification {
    /// Protocol name.
    pub protocol: String,
    /// Whether verification passed.
    pub passed: bool,
    /// Expected hash.
    pub expected: String,
    /// Actual hash (if computed).
    pub actual: Option<String>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl IdlVerification {
    fn passed(protocol: &str, hash: &str) -> Self {
        IdlVerification {
            protocol: protocol.to_string(),
            passed: true,
            expected: hash.to_string(),
            actual: Some(hash.to_string()),
            error: None,
        }
    }

    fn failed(protocol: &str, expected: &str, actual: &str) -> Self {
        IdlVerification {
            protocol: protocol.to_string(),
            passed: false,
            expected: expected.to_string(),
            actual: Some(actual.to_string()),
            error: Some(format!(
                "Hash mismatch: expected {}, got {}",
                expected, actual
            )),
        }
    }

    #[cfg(feature = "idl-verify")]
    fn missing(protocol: &str, expected: &str) -> Self {
        IdlVerification {
            protocol: protocol.to_string(),
            passed: false,
            expected: expected.to_string(),
            actual: None,
            error: Some("IDL file not found".to_string()),
        }
    }
}

/// Calculate SHA-256 hash of content.
pub fn sha256_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// Verify a single IDL file against expected hash.
pub fn verify_idl(protocol: &str, idl_content: &[u8], expected_hash: &str) -> IdlVerification {
    let actual_hash = sha256_hash(idl_content);

    if actual_hash == expected_hash {
        IdlVerification::passed(protocol, expected_hash)
    } else {
        IdlVerification::failed(protocol, expected_hash, &actual_hash)
    }
}

/// Get expected hash for a protocol.
pub fn get_expected_hash(protocol: &str) -> Option<&'static str> {
    EXPECTED_IDL_HASHES
        .iter()
        .find(|(name, _)| *name == protocol)
        .map(|(_, hash)| *hash)
}

/// Build a map of protocol name to expected hash.
pub fn expected_hashes_map() -> HashMap<&'static str, &'static str> {
    EXPECTED_IDL_HASHES.iter().copied().collect()
}

/// Verify all IDLs at build time.
///
/// This function should be called from build.rs or as a test.
/// It will panic if any IDL hash doesn't match.
///
/// # Panics
///
/// Panics if any IDL verification fails, with details about which
/// protocols need updating.
#[cfg(feature = "idl-verify")]
pub fn verify_all_idls(idl_dir: &std::path::Path) -> Result<Vec<IdlVerification>> {
    let mut results = Vec::new();
    let mut failures = Vec::new();

    for (protocol, expected_hash) in EXPECTED_IDL_HASHES {
        let idl_path = idl_dir.join(format!("{}.json", protocol));

        let verification = match std::fs::read(&idl_path) {
            Ok(content) => verify_idl(protocol, &content, expected_hash),
            Err(_) => IdlVerification::missing(protocol, expected_hash),
        };

        if !verification.passed {
            failures.push(verification.clone());
        }

        results.push(verification);
    }

    if !failures.is_empty() {
        let msg = failures
            .iter()
            .map(|v| {
                format!(
                    "  {}: {}",
                    v.protocol,
                    v.error.as_deref().unwrap_or("unknown")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        return Err(Error::protocol_error(
            "idl-verification",
            format!("IDL verification failed:\n{}", msg),
        ));
    }

    Ok(results)
}

/// Get the protocol name for IDL lookup.
impl Protocol {
    /// Get the IDL file name for this protocol.
    #[must_use]
    pub fn idl_name(&self) -> &'static str {
        match self {
            Protocol::Pumpfun => "pumpfun",
            Protocol::PumpSwap => "pumpswap",
            Protocol::RaydiumV4 => "raydium_v4",
            Protocol::RaydiumClmm => "raydium_clmm",
            Protocol::RaydiumCpmm => "raydium_cpmm",
            Protocol::RaydiumLaunchpad => "raydium_launchpad",
            Protocol::MeteoraDlmm => "meteora_dlmm",
            Protocol::MeteoraDbC => "meteora_dbc",
            Protocol::MeteoraDammV2 => "meteora_damm_v2",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hash_works() {
        let hash = sha256_hash(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn verify_idl_passes_on_match() {
        let content = b"test content";
        let expected = sha256_hash(content);

        let result = verify_idl("test", content, &expected);
        assert!(result.passed);
    }

    #[test]
    fn verify_idl_fails_on_mismatch() {
        let content = b"test content";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = verify_idl("test", content, wrong_hash);
        assert!(!result.passed);
        assert!(result.error.is_some());
    }

    #[test]
    fn expected_hashes_map_works() {
        let map = expected_hashes_map();
        assert!(map.contains_key("pumpfun"));
    }
}
