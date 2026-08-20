//! Unified error types for all protocols.
//!
//! Each protocol can have specific error variants, but all errors
//! are wrapped in the unified [`enum@Error`] type for consistent handling.

use solana_program::pubkey::Pubkey;
use thiserror::Error;

/// Result type alias using our Error.
pub type Result<T> = std::result::Result<T, Error>;

/// Unified error type for all protocol operations.
#[derive(Debug, Error)]
pub enum Error {
    // ========== Account Parsing Errors ==========
    /// Account data is invalid or malformed.
    #[error("invalid account data: {message}")]
    InvalidAccountData { message: String },

    /// Account discriminator doesn't match expected value.
    #[error("discriminator mismatch: expected {expected}, got {actual}")]
    DiscriminatorMismatch { expected: String, actual: String },

    /// Account data is too short.
    #[error("account data too short: need {expected} bytes, got {actual}")]
    AccountDataTooShort { expected: usize, actual: usize },

    // ========== Swap Calculation Errors ==========
    /// Swap amount is zero.
    #[error("swap amount cannot be zero")]
    ZeroAmount,

    /// The quote ran out of liquidity data before it ran out of swap.
    ///
    /// Distinct from a dead pool: a tick- or bin-based AMM needs the arrays
    /// its walk crosses, and how many that is depends on the trade size. A
    /// large swap can exhaust the arrays we hold while the pool is perfectly
    /// healthy. Truncating there and returning the partial answer would be a
    /// plausible, wrong number — worse than refusing, since nothing downstream
    /// could tell it apart from a real quote.
    ///
    /// No constant-product protocol can produce this; it exists so that adding
    /// a CLMM or DLMM is not a breaking change across every quote call site.
    #[error("quote exhausted the cached liquidity data before completing the swap")]
    InsufficientLiquidityData,

    /// Insufficient reserves for the requested swap.
    #[error("insufficient reserves: need {needed}, have {available}")]
    InsufficientReserves { needed: u64, available: u64 },

    /// Swap would cause excessive price impact.
    #[error("price impact too high: {impact_bps} bps exceeds max {max_bps} bps")]
    ExcessivePriceImpact { impact_bps: u16, max_bps: u16 },

    /// Arithmetic overflow in swap calculation.
    #[error("arithmetic overflow in swap calculation")]
    ArithmeticOverflow,

    /// The trade's gross output does not cover its own fees.
    ///
    /// Reachable on dust: every fee component rounds *up*, so a swap whose
    /// gross output is a few units can owe more in fees than it produces. The
    /// chain rejects such a trade, so a quote must refuse it — subtracting
    /// anyway underflows, which panics in debug and wraps to a near-`u64::MAX`
    /// output in release.
    #[error("trade too small: gross output {gross_out} does not cover fees {total_fee}")]
    FeesExceedOutput { gross_out: u64, total_fee: u64 },

    // ========== Protocol-Specific Errors ==========
    /// Pumpfun bonding curve has completed (graduated to AMM).
    #[error("bonding curve has completed (graduated to AMM)")]
    BondingCurveCompleted,

    /// Pool is paused or disabled.
    #[error("pool is paused: {reason}")]
    PoolPaused { reason: String },

    /// Pool doesn't exist at expected address.
    #[error("pool not found at {address}")]
    PoolNotFound { address: Pubkey },

    // ========== Instruction Building Errors ==========
    /// Missing required account for instruction.
    #[error("missing required account: {name}")]
    MissingAccount { name: String },

    /// Invalid pubkey provided.
    #[error("invalid pubkey for {field}: {reason}")]
    InvalidPubkey { field: String, reason: String },

    // ========== IDL Verification Errors ==========
    /// IDL hash mismatch - protocol may have been updated.
    #[error("IDL hash mismatch for {protocol}: expected {expected}, got {actual}")]
    IdlHashMismatch {
        protocol: String,
        expected: String,
        actual: String,
    },

    /// IDL file not found.
    #[error("IDL file not found: {path}")]
    IdlNotFound { path: String },

    // ========== Generic Errors ==========
    /// Protocol-specific error with context.
    #[error("{protocol} error: {message}")]
    ProtocolError { protocol: String, message: String },

    /// Wrapped anyhow error for flexibility.
    #[error(transparent)]
    Other(#[from] anyhow::Error),

    /// `Box<dyn Error>` for macro-generated parsers.
    #[error("{0}")]
    BoxedError(String),
}

impl From<Box<dyn std::error::Error + Send + Sync>> for Error {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error::BoxedError(err.to_string())
    }
}

impl Error {
    /// Create an InvalidAccountData error with a message.
    pub fn invalid_account_data(message: impl Into<String>) -> Self {
        Error::InvalidAccountData {
            message: message.into(),
        }
    }

    /// Create a ProtocolError for a specific protocol.
    pub fn protocol_error(protocol: impl Into<String>, message: impl Into<String>) -> Self {
        Error::ProtocolError {
            protocol: protocol.into(),
            message: message.into(),
        }
    }

    /// Create a discriminator mismatch error.
    pub fn discriminator_mismatch(expected: &[u8], actual: &[u8]) -> Self {
        Error::DiscriminatorMismatch {
            expected: hex::encode(expected),
            actual: hex::encode(actual),
        }
    }

    /// Create a parse error (alias for invalid_account_data).
    pub fn parse_error(message: impl Into<String>) -> Self {
        Error::InvalidAccountData {
            message: message.into(),
        }
    }

    /// Create a swap calculation error.
    pub fn swap_error(message: impl Into<String>) -> Self {
        Error::ProtocolError {
            protocol: "swap".into(),
            message: message.into(),
        }
    }

    /// Check if this is a recoverable error (can retry).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::InsufficientReserves { .. } | Error::ExcessivePriceImpact { .. }
        )
    }

    /// Check if this is a permanent error (pool state issue).
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Error::BondingCurveCompleted | Error::PoolNotFound { .. } | Error::PoolPaused { .. }
        )
    }
}

/// Adapt a layout decode failure into this crate's error type.
///
/// Written once as a `From` impl rather than four times as a `map_err` closure:
/// the same match had been copy-pasted into every `state.rs`, so adding a
/// variant to [`AccountParseError`] meant finding all of them, and the copies
/// had already begun to disagree about which variants they named explicitly.
///
/// Exhaustive on purpose — no `_` arm. A new decode failure should break this
/// one site and force a deliberate answer about how it surfaces, which is the
/// whole reason the variants are typed.
///
/// [`AccountParseError`]: crate::parsing::state::AccountParseError
impl From<crate::parsing::state::AccountParseError> for Error {
    fn from(e: crate::parsing::state::AccountParseError) -> Self {
        use crate::parsing::state::AccountParseError as A;
        match e {
            A::TooShort { len, need } => Error::AccountDataTooShort {
                expected: need,
                actual: len,
            },
            A::Discriminator | A::TruncatedVersion { .. } | A::Malformed { .. } => {
                Error::invalid_account_data(e.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = Error::invalid_account_data("bad data");
        assert!(err.to_string().contains("bad data"));

        let err = Error::discriminator_mismatch(&[0x01, 0x02], &[0x03, 0x04]);
        assert!(err.to_string().contains("0102"));
        assert!(err.to_string().contains("0304"));
    }

    #[test]
    fn error_classification() {
        assert!(Error::InsufficientReserves {
            needed: 100,
            available: 50
        }
        .is_recoverable());

        assert!(Error::BondingCurveCompleted.is_permanent());
        assert!(!Error::BondingCurveCompleted.is_recoverable());
    }
}
