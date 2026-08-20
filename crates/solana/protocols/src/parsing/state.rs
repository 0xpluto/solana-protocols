//! The contract an on-chain account struct fulfils.
//!
//! An account struct models bytes that exist on chain: its fields are the
//! account's fields, in the account's order, and nothing else. Behaviour hangs
//! off it in an `impl`, the way Anchor programs are written. [`OnchainState`]
//! is that contract, and `#[derive(OnchainState)]` is how a type satisfies it —
//! by generating the parse from the field list, so a field the account does not
//! have cannot quietly exist.
//!
//! The single sanctioned divergence from the current on-chain type is a
//! *version-added* field: a program upgrade appends to an account, and older
//! accounts end early. Those are trailing `Option<T>`, where `None` means the
//! account predates the field — a different fact from any default value, and
//! deliberately never collapsed into one.

/// A field that exists only in accounts written after some program upgrade.
///
/// Deliberately **not** `Option<T>`. The two variants are not the valuable
/// part — `Option`'s twenty-odd combinators are, and nearly all of them are an
/// ergonomic route to the one thing that must not happen here:
/// `unwrap_or(false)`, `unwrap_or_default()`, `map_or(false, …)` each answer a
/// question the data cannot answer, silently. An unmigrated account and a
/// migrated account whose flag is off are different facts; a decoder that
/// returns `false` for both has destroyed the distinction before any caller
/// could act on it.
///
/// So this type carries no accessors at all. Reading it means `match` or
/// `if let`, which forces the call site to state what absence means *for it*
/// — which is the only place that question has an answer.
///
/// ```text
/// match curve.is_mayhem_mode {
///     Legacy::Present(true)  => …,
///     Legacy::Present(false) => …,
///     Legacy::Absent         => …,  // predates the cashback upgrade
/// }
/// ```
///
/// This does not make the collapse impossible — `Legacy::Absent => false` is
/// still writable. It makes it explicit, and visible in review, instead of a
/// four-character method call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(untagged)]
pub enum Legacy<T> {
    /// The account carries this field.
    Present(T),
    /// The account predates the upgrade that added it. Not a default, not a
    /// zero — the bytes are simply not there.
    Absent,
}

impl<T> Legacy<T> {
    /// Whether the field was there to read.
    ///
    /// Deliberately not `Option`-shaped: `Legacy` exists so that "the account
    /// predates this field" cannot collapse into "the field is false", and
    /// `is_some()` is the ergonomic path to exactly that collapse. This answers
    /// only the question the version-group check asks.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }
}

impl<T: borsh::BorshDeserialize> borsh::BorshDeserialize for Legacy<T> {
    /// Read the field if the account still has bytes for it, otherwise
    /// [`Absent`](Self::Absent).
    ///
    /// # This is only correct where the account is not padded
    ///
    /// Solana allocates accounts at or above their data size — PumpSwap pools
    /// arrive at 261, 300 and 301 bytes over the same 244-byte field span — and
    /// padding is bytes, indistinguishable from a field holding zero. So EOF is
    /// a *sufficient* signal for absence and not a necessary one: a
    /// version-added field sitting in front of padding reads `Present(0)`, which
    /// is exactly the absent-equals-false collapse this type exists to prevent.
    ///
    /// The `OnchainState` derive therefore does not rely on this impl for
    /// version groups; it keeps an explicit length threshold, which is the only
    /// thing that can tell a short account from a padded one. This impl exists
    /// so `Legacy<T>` composes inside plain borsh structs where the payload is
    /// exact — event bodies — and it is documented here so nobody reaches for it
    /// on an account and quietly gets the wrong answer.
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        use std::io::Read as _;
        let mut probe = [0u8; 1];
        match reader.read(&mut probe)? {
            0 => Ok(Self::Absent),
            _ => {
                let mut chained = probe.as_slice().chain(reader.by_ref());
                T::deserialize_reader(&mut chained).map(Self::Present)
            }
        }
    }
}

/// Why raw account bytes could not be read as this type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccountParseError {
    /// Fewer bytes than the required (non-version-added) fields occupy.
    #[error("account data too short: {len} bytes, need at least {need}")]
    TooShort { len: usize, need: usize },

    /// The account's prefix is not this type's discriminator.
    #[error("discriminator mismatch")]
    Discriminator,

    /// borsh refused the field bytes.
    ///
    /// Distinct from [`TooShort`](Self::TooShort), which is a length judgement
    /// made before decoding: this is the codec itself rejecting what it read —
    /// a `bool` outside `{0, 1}`, a length prefix past the end of the buffer.
    #[error("account fields did not decode: {reason}")]
    Malformed {
        /// What borsh said.
        reason: String,
    },

    /// Part of a version group's bytes are present but not all of it.
    ///
    /// Distinct from an older account, which carries *none* of the group.
    /// Fields added in one upgrade arrive together, so a partial block means
    /// truncated or corrupt data — reading the bytes that happen to be there
    /// would invent a version that never shipped.
    #[error("version group `{group}` truncated: {have} of {need} bytes")]
    TruncatedVersion {
        group: &'static str,
        have: usize,
        need: usize,
    },
}

/// A struct that *is* an on-chain account's layout.
///
/// Implemented by `#[derive(OnchainState)]`; hand-implementing it is a smell,
/// because the point is that the field list and the byte layout are one thing.
pub trait OnchainState: borsh::BorshDeserialize + Sized {
    /// Bytes every non-version-added field occupies, including any
    /// discriminator. Derived by summing the field types — never transcribed,
    /// which is how `POOL_ACCOUNT_SIZE = 301` came to reject the majority of
    /// real PumpSwap pools whose field span is 244.
    const REQUIRED_LEN: usize;

    /// Read the account. Version-added fields absent from `data` come back
    /// `None`, never defaulted.
    fn from_account_data(data: &[u8]) -> Result<Self, AccountParseError>;

    /// Decode the fields after the discriminator, borsh, **prefix read**.
    ///
    /// # Why not `try_from_slice`
    ///
    /// Instruction data is exactly what the sender wrote, so refusing trailing
    /// bytes there is correct. An account is not: Solana allocates it at or
    /// above its data size, and the difference is padding. Measured on real
    /// mainnet accounts, PumpSwap pools arrive at 261, 300 and 301 bytes over
    /// the same 244-byte field span — `try_from_slice` would reject the
    /// majority of live pools.
    ///
    /// So the account path reads a prefix and leaves the remainder, while the
    /// instruction path is strict. One codec, two entry points, and the
    /// difference is a property of the data rather than a concession.
    ///
    /// # Errors
    ///
    /// The bytes are not a valid borsh encoding of this type's fields.
    fn borsh_fields(after_discriminator: &[u8]) -> Result<Self, AccountParseError> {
        let mut cursor = after_discriminator;
        Self::deserialize(&mut cursor).map_err(|e| AccountParseError::Malformed {
            reason: e.to_string(),
        })
    }
}
