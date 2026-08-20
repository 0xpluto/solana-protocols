//! Parsing traits for macro-generated code.
//!
//! These traits are implemented by protocol types to enable generic parsing.
//! The derive macros generate implementations automatically.

use solana_program::pubkey::Pubkey;

use super::InstructionParseError;
/// Marker: this type's arguments are borsh, and nothing else.
///
/// Implemented by `#[derive(InstructionData)]`. It carries no methods on
/// purpose — decoding lives on [`FromInstructionData`], which is
/// **blanket-implemented** for every type with this marker, so there is exactly
/// one decoder and it is borsh's.
///
/// # Why the split
///
/// This used to be one trait whose `from_instruction_data` was a *provided*
/// method. A provided method is a default, not a rule: five params structs
/// quietly replaced it with `from_le_bytes` at literal offsets behind a
/// `data.len() < N` check — a *minimum*, so trailing bytes were discarded rather
/// than refused. That is how an undeclared `track_volume` rode along unnoticed,
/// and it is invisible to any test, because the struct owned the decoder the
/// test would have called.
///
/// Hand-writing a decoder is now a compile error: an `impl FromInstructionData`
/// collides with the blanket impl, and rustc says so in those words.
pub trait InstructionParams:
    borsh::BorshDeserialize + borsh::BorshSerialize + Sized
{
}

/// Private supertrait: the seal.
///
/// A blanket impl alone is *not* enforcement. rustc permits a concrete
/// `impl FromInstructionData for Foo` whenever it can prove `Foo` does not
/// satisfy the blanket's bound — so a struct simply skipped the marker and kept
/// its hand-rolled decoder, which is exactly what five of them had done. The
/// seal closes that: implementing [`FromInstructionData`] requires this trait,
/// and this trait is not nameable outside this module.
mod sealed {
    /// Implemented only for types carrying the borsh marker.
    pub trait Sealed {}
    impl<T: super::InstructionParams> Sealed for T {}
}

/// Decode an instruction's arguments.
///
/// **Cannot be implemented by hand.** It is sealed on [`InstructionParams`], so
/// the borsh blanket impl below is the only one that can exist; a hand-written
/// decoder fails to compile with `the trait bound ...: Sealed is not satisfied`.
pub trait FromInstructionData: sealed::Sealed + Sized {
    /// Decode arguments (no discriminator) from instruction data.
    ///
    /// Strict: `try_from_slice` refuses leftovers. Instruction data is exactly
    /// what the sender wrote — unlike an account, which Solana allocates at or
    /// above its data size — so a byte past the last field means the program
    /// grew an argument we do not model.
    ///
    /// A genuinely variable tail is expressed as a final
    /// [`OptionBool`](crate::protocols::OptionBool) field, which consumes it.
    ///
    /// # Errors
    ///
    /// The bytes are not a valid borsh encoding of this type, or carry more than
    /// it accounts for.
    fn from_instruction_data(data: &[u8]) -> Result<Self, InstructionParseError>;
}

impl<T: InstructionParams> FromInstructionData for T {
    fn from_instruction_data(data: &[u8]) -> Result<Self, InstructionParseError> {
        Self::try_from_slice(data).map_err(|e| {
            InstructionParseError::DeserializationFailed(format!(
                "{}: {e}",
                std::any::type_name::<Self>()
            ))
        })
    }
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

// The two `macro_rules!` that used to live here generated hand-written
// `FromInstructionData` impls — one wrapping borsh, one doing a `data.len() <
// $size` check and ignoring the remainder. Zero call sites, and the second is
// the exact minimum-length shape that let trailing arguments pass unnoticed.
// Deleted rather than left: a sealed trait with a public macro that punches
// through it is a gate with a door beside it.

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

/// Arguments for an instruction that declares none.
///
/// Not `()`: a unit type has no natural [`FromInstructionData`] impl, and the
/// one thing this needs to do is *refuse* a non-empty body. Trailing bytes on a
/// zero-argument instruction mean the program grew an argument we have not
/// noticed, which is exactly the change that should announce itself rather than
/// being skipped over.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
)]
pub struct NoParams;

impl NoParams {
    /// The argument encoding: empty, because there are no arguments.
    ///
    /// Present so a zero-argument variant satisfies the same
    /// encode/decode surface as its siblings; the discriminator its caller
    /// prepends is the whole instruction.
    #[must_use]
    pub fn to_data(&self) -> Vec<u8> {
        Vec::new()
    }
}

/// `NoParams` decodes through the same blanket impl as everything else.
///
/// It hand-wrote its own `from_instruction_data` too. borsh on a unit struct
/// accepts exactly zero bytes and refuses any trailing byte — the identical
/// semantics, from the one codec.
impl InstructionParams for NoParams {}

#[cfg(test)]
mod no_params_tests {
    use super::*;

    #[test]
    fn no_arguments_means_no_bytes() {
        assert!(NoParams::from_instruction_data(&[]).is_ok());
        assert!(NoParams::from_instruction_data(&[0]).is_err());
    }
}
