//! `track_volume` — an argument whose on-chain encoding is not consistent.
//!
//! The IDL types it as `OptionBool`, which it defines as
//! `{"kind": "struct", "fields": ["bool"]}` — a struct wrapping a `bool`, so
//! the canonical wire form is a **single byte**. Senders do not agree. Observed
//! on mainnet: the argument omitted entirely, the canonical one byte, and a
//! two-byte form that is borsh's `Option<bool>` encoding — three conventions
//! for one argument, plus a malformed pair.
//!
//! Every observed form is its own variant, deliberately. Normalising them into
//! `Option<bool>` would answer "what did the sender mean" — a question we
//! cannot answer for all of them and do not need to. A variant per byte string
//! makes encode/decode total and, more usefully, makes the distribution
//! *countable*: we can now measure which forms actually occur before deciding
//! whether any of them matter.
//!
//! It is not a cosmetic distinction. Setting the flag changes which accounts
//! the instruction expects — the volume accumulators — so a builder that
//! guesses the encoding can produce an instruction whose account list does not
//! match its own arguments.
//!
//! Applies to `buy` and `buy_exact_sol_in` (both 16 accounts, volume
//! accumulators at slots 12–13). The v2 buys do **not** declare it in either
//! the vendored or the live IDL, despite 25-byte `buy_exact_quote_in_v2`
//! instructions appearing on chain alongside a 28th account — that pairing is
//! real and measured, but it is not this argument, and its cause is open.

/// The wire forms of `track_volume` observed on mainnet.
///
/// Each variant is exactly one byte string. `None` is the argument being
/// absent, which is distinct from it being present and false.
#[derive(Default, PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub enum TrackVolume {
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

impl TrackVolume {
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
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, UnknownTrackVolume> {
        match bytes {
            [] => Ok(Self::None),
            [0] => Ok(Self::SomeFalse),
            [1] => Ok(Self::SomeTrue),
            [1, 1] => Ok(Self::SomeTrueExtra),
            [1, 0] => Ok(Self::SomeFalseExtra),
            [0, 1] => Ok(Self::SomeFalseExtraInvalid),
            other => Err(UnknownTrackVolume(other.to_vec())),
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
pub struct UnknownTrackVolume(pub Vec<u8>);

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant round-trips through its exact bytes, and no two variants
    /// share an encoding — which is what makes the byte string the identity.
    #[test]
    fn each_form_round_trips_and_is_distinct() {
        let all = [
            TrackVolume::None,
            TrackVolume::SomeFalse,
            TrackVolume::SomeTrue,
            TrackVolume::SomeTrueExtra,
            TrackVolume::SomeFalseExtra,
            TrackVolume::SomeFalseExtraInvalid,
        ];
        let mut seen = std::collections::HashSet::new();
        for v in all {
            assert_eq!(TrackVolume::from_bytes(v.to_bytes()), Ok(v));
            assert!(seen.insert(v.to_bytes()), "{v:?} shares an encoding");
        }
        assert_eq!(seen.len(), 6);
    }

    /// An absent argument is not a false one. Collapsing them is the same
    /// mistake `Legacy<T>` exists to prevent, one layer down.
    #[test]
    fn absent_is_not_false() {
        assert_ne!(TrackVolume::None, TrackVolume::SomeFalse);
        assert_eq!(TrackVolume::None.requested(), None);
        assert_eq!(TrackVolume::SomeFalse.requested(), Some(false));
    }

    /// The ambiguous form refuses to answer rather than picking a reading.
    #[test]
    fn the_ambiguous_form_declines_to_resolve() {
        assert_eq!(TrackVolume::SomeFalseExtra.requested(), None);
        assert_eq!(TrackVolume::SomeFalseExtra.to_bytes(), &[1, 0]);
    }

    /// An unseen encoding errors rather than snapping to a neighbour.
    #[test]
    fn an_unseen_encoding_is_refused() {
        assert_eq!(
            TrackVolume::from_bytes(&[2]),
            Err(UnknownTrackVolume(vec![2]))
        );
        assert!(TrackVolume::from_bytes(&[1, 1, 1]).is_err());
    }
}
