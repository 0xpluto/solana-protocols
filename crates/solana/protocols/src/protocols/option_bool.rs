//! `OptionBool` — a pump-family argument whose on-chain encoding is not
//! consistent.
//!
//! Named after the type the programs themselves declare. Both `pump.json` and
//! `pump_amm.json` define it as `{"kind":"struct","fields":["bool"]}` — a
//! struct wrapping a `bool`, so the canonical wire form is a **single byte**.
//! It is *not* borsh's `Option<bool>`, and reading it as one is what stopped
//! pumpfun's `create_v2` from ever parsing a real instruction.
//!
//! Senders do not agree on the encoding. Observed on mainnet: the argument
//! omitted entirely, the canonical one byte, and a two-byte form that is
//! borsh's `Option<bool>` — three conventions for one argument, plus a
//! malformed pair.
//!
//! # Why it lives here and not under one protocol
//!
//! Measured, not assumed: `OptionBool` appears in exactly two of the vendored
//! IDLs, pumpfun and pumpswap, across six instructions — pumpfun `buy`,
//! `buy_exact_sol_in`, `create_v2`; pumpswap `buy`, `buy_exact_quote_in`,
//! `create_pool`. No other vendored program declares it. Other protocols do
//! carry trailing defined-type arguments (`SwapParameters`,
//! `ConfigParameters`, `Fees`) but those are ordinary parameter structs, not
//! optional-bool trailers.
//!
//! So this is a **pump-family convention, not a Solana-wide one**. It sits at
//! `protocols::` because two sibling protocols share it, and it is deliberately
//! not generalised further than the evidence supports.
//!
//! # Why every form is its own variant
//!
//! Normalising them into `Option<bool>` would answer "what did the sender
//! mean", which for one form cannot be answered. A variant per byte string
//! makes encode/decode total and, more usefully, makes the distribution
//! *countable*: the encoding a sender chose is a fingerprint of their tooling,
//! so an anomaly here is data rather than an error.
//!
//! It is not cosmetic, and this is measured rather than assumed. Over 1,050 v2
//! instructions captured from the firehose 2026-08-12:
//!
//! ```text
//! buy_exact_quote_in_v2   24 bytes, 27 accounts, no trailing   x362
//! buy_exact_quote_in_v2   25 bytes, 28 accounts, trailing [1]  x113
//! ```
//!
//! The correlation is perfect: the flag present and true adds an account. So
//! the argument changes the account list the program requires, and a builder
//! that guesses the encoding can emit an instruction whose accounts contradict
//! its own arguments.
//!
//! Two further facts fall out of that sample:
//!
//! * The trailing byte is **always `[1]`**, never `[0]`. Sending false is
//!   equivalent to omitting the argument, so clients omit it — which is why
//!   the absent form dominates and why absent must not be read as false.
//! * `buy_exact_quote_in_v2` **does** take this argument, and neither the
//!   vendored nor the freshly-fetched on-chain IDL declares it. The IDL is
//!   incomplete for that instruction. Trailing bytes here are semantic, not
//!   junk: rejecting them outright would drop ~11% of these instructions and
//!   lose the flag they carry.

/// The wire forms of `track_volume` observed on mainnet.
///
/// Each variant is exactly one byte string. `None` is the argument being
/// absent, which is distinct from it being present and false.
#[derive(
    Default, PartialEq, Eq, Hash, Debug, Clone, Copy, serde::Serialize, serde::Deserialize,
)]
pub enum OptionBool {
    /// Argument omitted — serializes to `[]`.
    #[default]
    None,
    /// Canonical `OptionBool` false — `[0]`.
    SomeFalse,
    /// Canonical `OptionBool` true — `[1]`.
    SomeTrue,
    /// Borsh `Option<bool>` encoding of true — `[1, 1]`.
    SomeTrueExtra,
    /// Borsh `Option<bool>` encoding of false — `[1, 0]`.
    ///
    /// Genuinely ambiguous on the wire: under borsh `Option` this is
    /// `Some(false)`, but read as the canonical single byte plus a trailing
    /// byte it is `true` followed by junk. We record the bytes and decline to
    /// resolve it, because the two readings disagree about the flag's value
    /// and nothing we hold settles which the sender intended.
    SomeFalseExtra,
    /// `[0, 1]` — a false tag carrying a payload. Malformed under every
    /// reading; kept so it can be counted rather than silently rejected.
    SomeFalseExtraInvalid,
}

impl OptionBool {
    /// The exact bytes this form serializes to.
    #[must_use]
    pub const fn to_bytes(self) -> &'static [u8] {
        match self {
            Self::None => &[],
            Self::SomeFalse => &[0],
            Self::SomeTrue => &[1],
            Self::SomeTrueExtra => &[1, 1],
            Self::SomeFalseExtra => &[1, 0],
            Self::SomeFalseExtraInvalid => &[0, 1],
        }
    }

    /// Read the trailing bytes of an instruction's argument data.
    ///
    /// Returns `Err` on any byte string we have not observed, rather than
    /// snapping to the nearest variant: an unrecognised encoding is a fact
    /// about the chain worth surfacing, and guessing would hide the next
    /// convention the way collapsing these six would have hidden these.
    ///
    /// # Errors
    ///
    /// The bytes match no observed form.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, UnknownOptionBool> {
        match bytes {
            [] => Ok(Self::None),
            [0] => Ok(Self::SomeFalse),
            [1] => Ok(Self::SomeTrue),
            [1, 1] => Ok(Self::SomeTrueExtra),
            [1, 0] => Ok(Self::SomeFalseExtra),
            [0, 1] => Ok(Self::SomeFalseExtraInvalid),
            other => Err(UnknownOptionBool(other.to_vec())),
        }
    }

    /// Whether the instruction is asking for volume tracking, where that is
    /// answerable.
    ///
    /// `None` for the absent argument and for [`SomeFalseExtra`], whose two
    /// readings disagree — the caller decides what to do with an unresolved
    /// flag rather than being handed a fabricated `false`.
    ///
    /// [`SomeFalseExtra`]: Self::SomeFalseExtra
    #[must_use]
    pub const fn requested(self) -> Option<bool> {
        match self {
            Self::None | Self::SomeFalseExtra | Self::SomeFalseExtraInvalid => Option::None,
            Self::SomeFalse => Some(false),
            Self::SomeTrue | Self::SomeTrueExtra => Some(true),
        }
    }
}

/// A `track_volume` encoding we have never seen on chain.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unrecognised track_volume encoding: {0:?}")]
pub struct UnknownOptionBool(pub Vec<u8>);

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant round-trips through its exact bytes, and no two variants
    /// share an encoding — which is what makes the byte string the identity.
    #[test]
    fn each_form_round_trips_and_is_distinct() {
        let all = [
            OptionBool::None,
            OptionBool::SomeFalse,
            OptionBool::SomeTrue,
            OptionBool::SomeTrueExtra,
            OptionBool::SomeFalseExtra,
            OptionBool::SomeFalseExtraInvalid,
        ];
        let mut seen = std::collections::HashSet::new();
        for v in all {
            assert_eq!(OptionBool::from_bytes(v.to_bytes()), Ok(v));
            assert!(seen.insert(v.to_bytes()), "{v:?} shares an encoding");
        }
        assert_eq!(seen.len(), 6);
    }

    /// An absent argument is not a false one. Collapsing them is the same
    /// mistake `Legacy<T>` exists to prevent, one layer down.
    #[test]
    fn absent_is_not_false() {
        assert_ne!(OptionBool::None, OptionBool::SomeFalse);
        assert_eq!(OptionBool::None.requested(), None);
        assert_eq!(OptionBool::SomeFalse.requested(), Some(false));
    }

    /// The ambiguous form refuses to answer rather than picking a reading.
    #[test]
    fn the_ambiguous_form_declines_to_resolve() {
        assert_eq!(OptionBool::SomeFalseExtra.requested(), None);
        assert_eq!(OptionBool::SomeFalseExtra.to_bytes(), &[1, 0]);
    }

    /// An unseen encoding errors rather than snapping to a neighbour.
    #[test]
    fn an_unseen_encoding_is_refused() {
        assert_eq!(
            OptionBool::from_bytes(&[2]),
            Err(UnknownOptionBool(vec![2]))
        );
        assert!(OptionBool::from_bytes(&[1, 1, 1]).is_err());
    }
}
