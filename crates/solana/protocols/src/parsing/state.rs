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

/// Why raw account bytes could not be read as this type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccountParseError {
    /// Fewer bytes than the required (non-version-added) fields occupy.
    #[error("account data too short: {len} bytes, need at least {need}")]
    TooShort { len: usize, need: usize },

    /// The account's prefix is not this type's discriminator.
    #[error("discriminator mismatch")]
    Discriminator,

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
pub trait OnchainState: Sized {
    /// Bytes every non-version-added field occupies, including any
    /// discriminator. Derived by summing the field types — never transcribed,
    /// which is how `POOL_ACCOUNT_SIZE = 301` came to reject the majority of
    /// real PumpSwap pools whose field span is 244.
    const REQUIRED_LEN: usize;

    /// Read the account. Version-added fields absent from `data` come back
    /// `None`, never defaulted.
    fn from_account_data(data: &[u8]) -> Result<Self, AccountParseError>;
}
