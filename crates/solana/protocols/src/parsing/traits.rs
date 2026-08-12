//! Parsing traits for macro-generated code.
//!
//! These traits are implemented by protocol types to enable generic parsing.
//! The derive macros generate implementations automatically.

use solana_program::pubkey::Pubkey;

use super::InstructionParseError;

/// Trait for types that can be parsed from instruction data bytes.
///
/// Implement this for instruction parameter structs.
/// The `#[derive(InstructionData)]` macro generates this automatically.
pub trait FromInstructionData: Sized {
    /// Parse from instruction data bytes (after discriminator).
    fn from_instruction_data(data: &[u8]) -> Result<Self, InstructionParseError>;
}

/// Trait for types that can be constructed from account pubkeys.
///
/// Implement this for account builder structs.
/// The `#[derive(AccountMetas)]` macro can generate this with `from_pubkeys` attribute.
pub trait FromAccountKeys: Sized {
    /// Minimum number of accounts required.
    const MIN_ACCOUNTS: usize;

    /// Parse from a slice of account pubkeys.
    fn from_account_keys(keys: &[Pubkey]) -> Result<Self, InstructionParseError>;
}

/// Marker for an instruction accounts-struct produced by
/// `#[derive(OnchainInstruction)]`.
///
/// The parse side (`from_pubkeys`, used by the extractors) is silent-corruption-
/// prone: a wrong account order or `#[account(writable)]`/`#[account(signer)]`
/// annotation reads the wrong pubkey into the wrong role with no loud failure.
/// A `VerifiedInstruction` carries a golden fixture ([`FIXTURE`](Self::FIXTURE))
/// captured from a **real landed instruction**; the derive turns it into a
/// round-trip `#[test]` asserting `from_pubkeys(real).to_account_metas()`
/// reproduces the real account order and flags.
///
/// As with [`VerifiedDecoder`](solana_account_traits::VerifiedDecoder), this is not a
/// dispatch bound — non-Anchor instructions stay parseable — completeness is a
/// test asserting every fixtured instruction struct carries the marker.
pub trait VerifiedInstruction: FromAccountKeys {
    /// Path of the golden instruction fixture (relative to `fixtures/`).
    const FIXTURE: &'static str;
}

/// Trait for types that can be parsed from log data.
///
/// Implement this for log/event structs.
/// The `#[derive(LogParser)]` macro generates this automatically.
pub trait FromLogData: Sized {
    /// The log discriminator (first 8 bytes).
    const DISCRIMINATOR: [u8; 8];

    /// Parse from log data bytes (after discriminator).
    fn from_log_data(data: &[u8]) -> Result<Self, InstructionParseError>;

    /// Check if data starts with this type's discriminator.
    fn matches_discriminator(data: &[u8]) -> bool {
        data.len() >= 8 && data[..8] == Self::DISCRIMINATOR
    }
}

// =============================================================================
// Default implementations for common types
// =============================================================================

/// Implement FromInstructionData for types that use borsh.
#[macro_export]
macro_rules! impl_from_instruction_data_borsh {
    ($type:ty) => {
        impl $crate::parsing::FromInstructionData for $type {
            fn from_instruction_data(
                data: &[u8],
            ) -> Result<Self, $crate::parsing::InstructionParseError> {
                borsh::BorshDeserialize::try_from_slice(data).map_err(|e| {
                    $crate::parsing::InstructionParseError::DeserializationFailed(e.to_string())
                })
            }
        }
    };
}

/// Implement FromInstructionData for types with fixed-size binary layout.
#[macro_export]
macro_rules! impl_from_instruction_data_fixed {
    ($type:ty, $size:expr) => {
        impl $crate::parsing::FromInstructionData for $type {
            fn from_instruction_data(
                data: &[u8],
            ) -> Result<Self, $crate::parsing::InstructionParseError> {
                if data.len() < $size {
                    return Err(
                        $crate::parsing::InstructionParseError::DeserializationFailed(format!(
                            "expected {} bytes, got {}",
                            $size,
                            data.len()
                        )),
                    );
                }
                // Safety: We verified the length
                Ok(unsafe { std::ptr::read(data.as_ptr() as *const Self) })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_log_data_discriminator_check() {
        struct TestLog;

        impl FromLogData for TestLog {
            const DISCRIMINATOR: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

            fn from_log_data(_data: &[u8]) -> Result<Self, InstructionParseError> {
                Ok(TestLog)
            }
        }

        assert!(TestLog::matches_discriminator(&[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10
        ]));
        assert!(!TestLog::matches_discriminator(&[1, 2, 3, 4, 5, 6, 7, 9]));
        assert!(!TestLog::matches_discriminator(&[1, 2, 3, 4])); // Too short
    }
}
