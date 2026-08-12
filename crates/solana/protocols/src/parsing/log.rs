//! Log parsing utilities.
//!
//! This module provides improved log parsing with:
//! - Cached regex patterns (lazy_static)
//! - Discriminator extraction from "Program data:" logs
//! - Proper error types
//! - Support for all log variants
//!
//! # Log Types
//!
//! Solana runtime produces several log formats:
//! - `Program X invoke [N]` - Program invocation at depth N
//! - `Program X success` - Program completed successfully
//! - `Program log: <message>` - Custom program message
//! - `Program data: <base64>` - Program data (used for events)
//! - `Program return: X <base64>` - Program return data
//! - `Program X consumed N of M compute units` - Compute usage
//! - `Program X failed: <error>` - Program failure

use base64::{engine::general_purpose::STANDARD, Engine as _};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use std::str::FromStr;
use tracing::{debug, warn};

// =============================================================================
// Cached Regex Patterns
// =============================================================================

static INVOKE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) invoke \[(\d+)\]$").unwrap());

static SUCCESS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) success$").unwrap());

static RETURN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^Program return: ([1-9A-HJ-NP-Za-km-z]{32,44}) (.+)$").unwrap());

static CONSUMED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) consumed (\d+) of (\d+) compute units$")
        .unwrap()
});

static FAILED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) failed: (.+)$").unwrap());

// =============================================================================
// LogEntry Enum
// =============================================================================

/// Structured log entry from transaction execution.
///
/// Each variant captures the relevant data for that log type.
/// Use [`parse_logs`] to convert raw log strings to this type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogEntry {
    /// Program invocation: `"Program X invoke [N]"`
    Invoke {
        /// Program being invoked.
        program: Pubkey,
        /// CPI depth (1 = top-level).
        depth: u32,
    },

    /// Program success: `"Program X success"`
    Success {
        /// Program that succeeded.
        program: Pubkey,
    },

    /// Program data (event): `"Program data: <base64>"`
    ///
    /// The discriminator is the first 8 bytes, used for event type matching.
    Data {
        /// First 8 bytes of decoded data (event discriminator).
        discriminator: [u8; 8],
        /// Remaining bytes after discriminator.
        payload: Vec<u8>,
    },

    /// Program return: `"Program return: X <base64>"`
    Return {
        /// Program that returned data.
        program: Pubkey,
        /// Decoded return data.
        data: Vec<u8>,
    },

    /// Program log message: `"Program log: <message>"`
    Message {
        /// The log message content.
        message: String,
    },

    /// Compute consumption: `"Program X consumed N of M compute units"`
    Consumed {
        /// Program that consumed compute.
        program: Pubkey,
        /// Compute units used.
        used: u64,
        /// Compute units available.
        available: u64,
    },

    /// Program failure: `"Program X failed: <error>"`
    Failed {
        /// Program that failed.
        program: Pubkey,
        /// Error message.
        error: String,
    },

    /// Log truncation indicator.
    ///
    /// When transaction uses too much compute, logs may be truncated.
    Truncated,

    /// Unknown log format.
    Unknown {
        /// Raw log string.
        raw: String,
    },
}

/// Check if a string is clearly not valid base64.
///
/// Returns true when the data contains characters that are never part of base64
/// encoding (spaces, most punctuation, control chars). This identifies plaintext
/// program output that was emitted via the "Program data:" prefix but is not
/// actually base64-encoded data.
fn is_clearly_not_base64(data: &str) -> bool {
    // Base64 alphabet: A-Z, a-z, 0-9, +, /, = (padding)
    // Whitespace is never valid in standard base64
    data.contains(' ')
        || data.contains('\t')
        || data.contains(',')
        || data.contains(':')
        || data.contains(';')
        || data.contains('!')
        || data.contains('?')
        || data.contains('{')
        || data.contains('}')
        || data.contains('[')
        || data.contains(']')
        || data.contains('(')
        || data.contains(')')
        || data.contains('<')
        || data.contains('>')
        || data.contains('\'')
        || data.contains('"')
        || data.contains('\\')
        || data.contains('~')
        || data.contains('`')
        || data.contains('@')
        || data.contains('#')
        || data.contains('$')
        || data.contains('%')
        || data.contains('^')
        || data.contains('&')
        || data.contains('*')
        || data.contains('|')
}

/// Truncate a string for log output, appending "..." if truncated.
fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

impl LogEntry {
    /// Parse a single log string into a LogEntry.
    #[must_use]
    pub fn parse(log: &str) -> Self {
        // Check for truncation
        if log.contains("Log truncated") {
            return LogEntry::Truncated;
        }

        // Program data: <base64>
        if let Some(data_str) = log.strip_prefix("Program data: ") {
            return Self::parse_data_log(data_str);
        }

        // Program log: <message>
        if let Some(message) = log.strip_prefix("Program log: ") {
            return LogEntry::Message {
                message: message.to_string(),
            };
        }

        // Program return: X <base64>
        if let Some(caps) = RETURN_RE.captures(log) {
            if let (Some(program_str), Some(data_str)) = (caps.get(1), caps.get(2)) {
                if let Ok(program) = Pubkey::from_str(program_str.as_str()) {
                    let data = STANDARD.decode(data_str.as_str()).unwrap_or_default();
                    return LogEntry::Return { program, data };
                }
            }
        }

        // Program X invoke [N]
        if let Some(caps) = INVOKE_RE.captures(log) {
            if let (Some(program_str), Some(depth_str)) = (caps.get(1), caps.get(2)) {
                if let (Ok(program), Ok(depth)) = (
                    Pubkey::from_str(program_str.as_str()),
                    depth_str.as_str().parse::<u32>(),
                ) {
                    return LogEntry::Invoke { program, depth };
                }
            }
        }

        // Program X success
        if let Some(caps) = SUCCESS_RE.captures(log) {
            if let Some(program_str) = caps.get(1) {
                if let Ok(program) = Pubkey::from_str(program_str.as_str()) {
                    return LogEntry::Success { program };
                }
            }
        }

        // Program X consumed N of M compute units
        if let Some(caps) = CONSUMED_RE.captures(log) {
            if let (Some(program_str), Some(used_str), Some(available_str)) =
                (caps.get(1), caps.get(2), caps.get(3))
            {
                if let (Ok(program), Ok(used), Ok(available)) = (
                    Pubkey::from_str(program_str.as_str()),
                    used_str.as_str().parse::<u64>(),
                    available_str.as_str().parse::<u64>(),
                ) {
                    return LogEntry::Consumed {
                        program,
                        used,
                        available,
                    };
                }
            }
        }

        // Program X failed: <error>
        if let Some(caps) = FAILED_RE.captures(log) {
            if let (Some(program_str), Some(error_str)) = (caps.get(1), caps.get(2)) {
                if let Ok(program) = Pubkey::from_str(program_str.as_str()) {
                    return LogEntry::Failed {
                        program,
                        error: error_str.as_str().to_string(),
                    };
                }
            }
        }

        // Unknown format
        LogEntry::Unknown {
            raw: log.to_string(),
        }
    }

    /// Parse "Program data:" log content.
    fn parse_data_log(data_str: &str) -> Self {
        match STANDARD.decode(data_str) {
            Ok(bytes) if bytes.len() >= 8 => {
                let mut discriminator = [0u8; 8];
                discriminator.copy_from_slice(&bytes[..8]);
                let payload = bytes[8..].to_vec();
                LogEntry::Data {
                    discriminator,
                    payload,
                }
            }
            Ok(bytes) => {
                // Less than 8 bytes - still a Data log but with zero-padded discriminator
                let mut discriminator = [0u8; 8];
                discriminator[..bytes.len()].copy_from_slice(&bytes);
                LogEntry::Data {
                    discriminator,
                    payload: Vec::new(),
                }
            }
            Err(e) => {
                // Most decode failures are plaintext program output routed through
                // "Program data:" — e.g. "Synopsis ..." blobs with spaces. These
                // are benign and produce ~1500 WARN/hr. Downgrade to DEBUG when the
                // data is clearly not base64 (contains spaces or other non-base64
                // bytes), keeping WARN only for genuinely unexpected failures.
                if is_clearly_not_base64(data_str) {
                    debug!(
                        data_prefix = %truncate_for_log(data_str, 60),
                        error = %e,
                        "Skipping non-base64 program data log"
                    );
                } else {
                    warn!(data = %data_str, error = %e, "Failed to decode base64 log data");
                }
                LogEntry::Unknown {
                    raw: format!("Program data: {}", data_str),
                }
            }
        }
    }

    /// Check if this is an Invoke log.
    #[must_use]
    pub fn is_invoke(&self) -> bool {
        matches!(self, LogEntry::Invoke { .. })
    }

    /// Check if this is a Success log.
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, LogEntry::Success { .. })
    }

    /// Check if this is a Data log.
    #[must_use]
    pub fn is_data(&self) -> bool {
        matches!(self, LogEntry::Data { .. })
    }

    /// Check if this is a Message log.
    #[must_use]
    pub fn is_message(&self) -> bool {
        matches!(self, LogEntry::Message { .. })
    }

    /// Check if this is a Failed log.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self, LogEntry::Failed { .. })
    }

    /// Get the program ID if this log is associated with a specific program.
    #[must_use]
    pub fn program(&self) -> Option<&Pubkey> {
        match self {
            LogEntry::Invoke { program, .. }
            | LogEntry::Success { program }
            | LogEntry::Return { program, .. }
            | LogEntry::Consumed { program, .. }
            | LogEntry::Failed { program, .. } => Some(program),
            _ => None,
        }
    }
}

// =============================================================================
// Log Parser Trait
// =============================================================================

/// Trait for parsing typed events from log data.
///
/// Implement this for protocol-specific event types.
/// The `#[derive(LogParser)]` macro can generate this automatically.
pub trait LogParser: Sized {
    /// The discriminator bytes for this event type.
    const DISCRIMINATOR: [u8; 8];

    /// Parse from log payload (bytes after discriminator).
    fn from_log_payload(payload: &[u8]) -> Result<Self, LogParseError>;

    /// Parse from a LogEntry.
    fn from_log_entry(entry: &LogEntry) -> Result<Self, LogParseError> {
        match entry {
            LogEntry::Data {
                discriminator,
                payload,
            } => {
                if *discriminator != Self::DISCRIMINATOR {
                    return Err(LogParseError::DiscriminatorMismatch {
                        expected: Self::DISCRIMINATOR,
                        actual: *discriminator,
                    });
                }
                Self::from_log_payload(payload)
            }
            _ => Err(LogParseError::NotDataLog),
        }
    }
}

/// Error from parsing log data.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LogParseError {
    /// Discriminator doesn't match expected value.
    #[error("discriminator mismatch: expected {expected:?}, got {actual:?}")]
    DiscriminatorMismatch { expected: [u8; 8], actual: [u8; 8] },

    /// Log entry is not a Data type.
    #[error("log entry is not a Program data type")]
    NotDataLog,

    /// Payload is too short.
    #[error("payload too short: need {expected} bytes, got {actual}")]
    PayloadTooShort { expected: usize, actual: usize },

    /// Failed to deserialize payload.
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),
}

// =============================================================================
// Parsed Log (Legacy Compatibility)
// =============================================================================

/// Parsed log with associated raw string.
///
/// This provides compatibility with code expecting the raw log string.
#[derive(Debug, Clone)]
pub struct ParsedLog {
    /// The structured log entry.
    pub entry: LogEntry,
    /// The original raw log string.
    pub raw: String,
}

impl ParsedLog {
    /// Create from a raw log string.
    #[must_use]
    pub fn new(raw: String) -> Self {
        let entry = LogEntry::parse(&raw);
        Self { entry, raw }
    }
}

// =============================================================================
// Parsing Functions
// =============================================================================

/// Parse an iterator of log strings into LogEntry values.
///
/// This is the main entry point for log parsing.
pub fn parse_logs<'a, I>(logs: I) -> Vec<LogEntry>
where
    I: Iterator<Item = &'a str>,
{
    logs.map(LogEntry::parse).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_invoke_log() {
        let log = "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Invoke { program, depth } => {
                assert_eq!(
                    program.to_string(),
                    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
                );
                assert_eq!(depth, 1);
            }
            _ => panic!("Expected Invoke, got {:?}", entry),
        }
    }

    #[test]
    fn parse_success_log() {
        let log = "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P success";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Success { program } => {
                assert_eq!(
                    program.to_string(),
                    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
                );
            }
            _ => panic!("Expected Success, got {:?}", entry),
        }
    }

    #[test]
    fn parse_data_log() {
        // Base64 of [1,2,3,4,5,6,7,8,9,10]
        let log = "Program data: AQIDBAUGBwgJCg==";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Data {
                discriminator,
                payload,
            } => {
                assert_eq!(discriminator, [1, 2, 3, 4, 5, 6, 7, 8]);
                assert_eq!(payload, vec![9, 10]);
            }
            _ => panic!("Expected Data, got {:?}", entry),
        }
    }

    #[test]
    fn parse_message_log() {
        let log = "Program log: Hello, world!";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Message { message } => {
                assert_eq!(message, "Hello, world!");
            }
            _ => panic!("Expected Message, got {:?}", entry),
        }
    }

    #[test]
    fn parse_consumed_log() {
        let log = "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P consumed 12345 of 200000 compute units";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Consumed {
                program,
                used,
                available,
            } => {
                assert_eq!(
                    program.to_string(),
                    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
                );
                assert_eq!(used, 12345);
                assert_eq!(available, 200000);
            }
            _ => panic!("Expected Consumed, got {:?}", entry),
        }
    }

    #[test]
    fn parse_failed_log() {
        let log =
            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P failed: custom program error: 0x1";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Failed { program, error } => {
                assert_eq!(
                    program.to_string(),
                    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
                );
                assert_eq!(error, "custom program error: 0x1");
            }
            _ => panic!("Expected Failed, got {:?}", entry),
        }
    }

    #[test]
    fn parse_return_log() {
        // Base64 of [1,2,3,4]
        let log = "Program return: 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P AQIDBA==";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Return { program, data } => {
                assert_eq!(
                    program.to_string(),
                    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
                );
                assert_eq!(data, vec![1, 2, 3, 4]);
            }
            _ => panic!("Expected Return, got {:?}", entry),
        }
    }

    #[test]
    fn parse_unknown_log() {
        let log = "Some random log message";
        let entry = LogEntry::parse(log);

        match entry {
            LogEntry::Unknown { raw } => {
                assert_eq!(raw, "Some random log message");
            }
            _ => panic!("Expected Unknown, got {:?}", entry),
        }
    }

    #[test]
    fn log_entry_is_methods() {
        let invoke = LogEntry::Invoke {
            program: Pubkey::new_unique(),
            depth: 1,
        };
        assert!(invoke.is_invoke());
        assert!(!invoke.is_success());

        let success = LogEntry::Success {
            program: Pubkey::new_unique(),
        };
        assert!(success.is_success());
        assert!(!success.is_invoke());

        let data = LogEntry::Data {
            discriminator: [0; 8],
            payload: vec![],
        };
        assert!(data.is_data());

        let message = LogEntry::Message {
            message: "test".to_string(),
        };
        assert!(message.is_message());

        let failed = LogEntry::Failed {
            program: Pubkey::new_unique(),
            error: "error".to_string(),
        };
        assert!(failed.is_failed());
    }

    #[test]
    fn parse_logs_multiple() {
        let logs = [
            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]",
            "Program log: Hello",
            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P success",
        ];

        let entries = parse_logs(logs.iter().copied());
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_invoke());
        assert!(entries[1].is_message());
        assert!(entries[2].is_success());
    }

    // =========================================================================
    // Tests for is_clearly_not_base64
    // =========================================================================

    #[test]
    fn clearly_not_base64_with_spaces() {
        // "Synopsis" blobs — the primary spam pattern
        assert!(is_clearly_not_base64("Synopsis of the token launch event"));
        assert!(is_clearly_not_base64("some text with spaces"));
    }

    #[test]
    fn clearly_not_base64_with_punctuation() {
        assert!(is_clearly_not_base64("key:value"));
        assert!(is_clearly_not_base64("hello!"));
        assert!(is_clearly_not_base64("{json}"));
        assert!(is_clearly_not_base64("[array]"));
        assert!(is_clearly_not_base64("(parens)"));
        assert!(is_clearly_not_base64("has@symbol"));
        assert!(is_clearly_not_base64("dollar$sign"));
        assert!(is_clearly_not_base64("percent%encoded"));
        assert!(is_clearly_not_base64("pipe|char"));
    }

    #[test]
    fn valid_base64_not_flagged() {
        // Valid base64 strings should NOT be flagged as clearly-not-base64
        assert!(!is_clearly_not_base64("AQIDBAUGBwgJCg=="));
        assert!(!is_clearly_not_base64("dGVzdA=="));
        assert!(!is_clearly_not_base64("AAAAAAAAAAAAAAAAAAA="));
        // Even invalid base64 that uses only base64-alphabet chars should not be flagged
        // (those are genuinely unexpected failures that deserve WARN)
        assert!(!is_clearly_not_base64("notquitebase64butnospecialchars"));
    }

    #[test]
    fn empty_string_not_flagged() {
        assert!(!is_clearly_not_base64(""));
    }

    // =========================================================================
    // Tests for truncate_for_log
    // =========================================================================

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate_for_log("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_truncated() {
        assert_eq!(truncate_for_log("abcdefghij", 5), "abcde...");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate_for_log("abcde", 5), "abcde");
    }

    // =========================================================================
    // Tests for parse_data_log with non-base64 input
    // =========================================================================

    #[test]
    fn parse_data_log_with_synopsis_blob() {
        // Simulates the "Synopsis" spam pattern — should produce Unknown, not panic
        let log = "Program data: Synopsis of the token launch event details";
        let entry = LogEntry::parse(log);
        match entry {
            LogEntry::Unknown { raw } => {
                assert!(raw.contains("Synopsis"));
            }
            _ => panic!("Expected Unknown for non-base64 data, got {:?}", entry),
        }
    }

    #[test]
    fn parse_data_log_valid_base64_still_works() {
        // Valid base64 should still parse to Data
        let log = "Program data: AQIDBAUGBwgJCg==";
        let entry = LogEntry::parse(log);
        assert!(entry.is_data(), "Valid base64 should produce Data variant");
    }
}
