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
//! * On `buy_exact_quote_in_v2` the trailing byte is always `[1]`, never
//!   `[0]`. That is **not** true of the family: pumpfun's v1 `buy` sends all
//!   three forms — 945 absent, 477 `[0]`, 164 `[1]` in one 150s window. So
//!   `[0]` is genuinely used, and absent must not be read as false, and no
//!   single instruction's distribution generalises to the others.
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
    /// Bytes past the arguments matching no encoding we can attribute, kept
    /// verbatim.
    ///
    /// Observed on mainnet 2026-08-15: `buy_exact_sol_in` carrying eight zero
    /// bytes where the IDL declares one. The instruction is valid with the
    /// argument and valid without it, so the program accepted this — a decoder
    /// that refuses is wrong about the chain, not the other way round.
    ///
    /// What the bytes *mean* is genuinely open. `[0; 8]` reads as the canonical
    /// `[0]` plus seven of padding, as a `u64` zero, or as an undeclared
    /// argument leaving `track_volume` absent — and the first two say "false"
    /// while the third says "unset". So [`requested`](Self::requested) answers
    /// `None`: we record what arrived and decline to resolve it, exactly as for
    /// [`SomeFalseExtra`](Self::SomeFalseExtra).
    ///
    /// The bytes are the useful part. A sender's encoding is a fingerprint of
    /// its tooling, and an encoding nobody else emits is most likely one bot
    /// mis-serializing the argument — which the tape's `track_volume` column
    /// now carries as hex, so "who does this" is a query.
    Unattributed(Trailer),
}

/// Trailing bytes retained inline.
///
/// Bounded and `Copy` on purpose. `OptionBool` sits inside `Swap`, which sits
/// inside `ChainEvent`; growing a bottom-crate primitive by a heap pointer is
/// how `large_enum_variant` fired two crates up the last time, and the ruling
/// then was to shrink rather than allow. Sixteen bytes is twice the longest
/// trailer observed.
///
/// Anything longer is *refused* rather than truncated, so this type is always
/// lossless and [`OptionBool::to_bytes`] stays total — a builder can never emit
/// a trailer it only partly remembers. That refusal is also what keeps the
/// alarm: a genuinely new convention still surfaces as an undecoded sample with
/// its bytes, instead of disappearing into this variant.
#[derive(PartialEq, Eq, Hash, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Trailer {
    len: u8,
    bytes: [u8; Self::MAX],
}

impl Trailer {
    /// Longest trailer retained inline.
    pub const MAX: usize = 16;

    /// Keep `bytes`, or `None` if they exceed [`MAX`](Self::MAX).
    #[must_use]
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > Self::MAX {
            return None;
        }
        let mut buf = [0u8; Self::MAX];
        buf[..bytes.len()].copy_from_slice(bytes);
        // `len` is bounded by MAX just above.
        #[allow(clippy::cast_possible_truncation)]
        Some(Self {
            len: bytes.len() as u8,
            bytes: buf,
        })
    }

    /// The bytes, exactly as they arrived.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// Hex, because these end up in a tape column and in log lines where a decimal
/// byte array is unreadable and unsearchable.
impl std::fmt::Debug for Trailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.as_slice() {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Trailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl OptionBool {
    /// The exact bytes this form serializes to.
    ///
    /// Total: every variant, including [`Unattributed`](Self::Unattributed),
    /// can reproduce its own encoding, because a trailer too long to store is
    /// refused at decode rather than kept in part.
    #[must_use]
    pub fn to_bytes(&self) -> &[u8] {
        match self {
            Self::None => &[],
            Self::SomeFalse => &[0],
            Self::SomeTrue => &[1],
            Self::SomeTrueExtra => &[1, 1],
            Self::SomeFalseExtra => &[1, 0],
            Self::SomeFalseExtraInvalid => &[0, 1],
            Self::Unattributed(t) => t.as_slice(),
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
            other => Trailer::new(other)
                .map(Self::Unattributed)
                .ok_or_else(|| UnknownOptionBool(other.to_vec())),
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
    pub const fn requested(&self) -> Option<bool> {
        match self {
            Self::None
            | Self::SomeFalseExtra
            | Self::SomeFalseExtraInvalid
            | Self::Unattributed(_) => Option::None,
            Self::SomeFalse => Some(false),
            Self::SomeTrue | Self::SomeTrueExtra => Some(true),
        }
    }

    /// Trailing bytes we could not attribute, if any.
    ///
    /// The fingerprinting accessor: a sender emitting an encoding nobody else
    /// emits is identifiable by it.
    #[must_use]
    pub fn unattributed(&self) -> Option<&[u8]> {
        match self {
            Self::Unattributed(t) => Some(t.as_slice()),
            Self::None
            | Self::SomeFalse
            | Self::SomeTrue
            | Self::SomeTrueExtra
            | Self::SomeFalseExtra
            | Self::SomeFalseExtraInvalid => Option::None,
        }
    }
}

/// A `track_volume` trailer too long to retain inline (see [`Trailer::MAX`]).
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
            assert!(
                seen.insert(v.to_bytes().to_vec()),
                "{v:?} shares an encoding"
            );
        }
        assert_eq!(seen.len(), 6);
        // The seventh form carries its bytes, so it is covered by the
        // round-trip test rather than this fixed list.
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

    /// An unattributed trailer is kept verbatim, not snapped to a neighbour.
    #[test]
    fn an_unattributed_trailer_is_kept_not_guessed() {
        let v = OptionBool::from_bytes(&[2]).expect("short trailer is retained");
        assert_eq!(v.unattributed(), Some([2].as_slice()));
        assert_eq!(v.to_bytes(), &[2]);
        assert_eq!(
            OptionBool::from_bytes(&[1, 1, 1]).unwrap().to_bytes(),
            &[1, 1, 1]
        );
    }

    /// The exact bytes a mainnet `buy_exact_sol_in` carried on 2026-08-15:
    /// eight zero bytes where the IDL declares a one-byte `OptionBool`. The
    /// program accepted it, so the decoder must too.
    #[test]
    fn the_observed_eight_zero_byte_trailer_decodes() {
        let v = OptionBool::from_bytes(&[0; 8]).expect("mainnet trailer decodes");
        assert_eq!(v.unattributed(), Some([0u8; 8].as_slice()));
        assert_eq!(format!("{v:?}"), "Unattributed(0000000000000000)");
    }

    /// Eight zero bytes read as `false` under two of three readings and as
    /// "absent" under the third. Answering `Some(false)` would pick one, which
    /// is the absent-equals-false collapse wearing a different hat.
    #[test]
    fn an_unattributed_trailer_never_resolves_the_flag() {
        assert_eq!(OptionBool::from_bytes(&[0; 8]).unwrap().requested(), None);
        assert_eq!(OptionBool::from_bytes(&[2]).unwrap().requested(), None);
    }

    /// Past the inline bound we refuse rather than truncate: a partly-kept
    /// trailer would make `to_bytes` lie, and refusing is what keeps a
    /// genuinely new convention surfacing as an undecoded sample.
    #[test]
    fn a_trailer_past_the_bound_is_refused_not_truncated() {
        let long = vec![9u8; Trailer::MAX + 1];
        assert_eq!(
            OptionBool::from_bytes(&long),
            Err(UnknownOptionBool(long.clone()))
        );
        let at_bound = vec![9u8; Trailer::MAX];
        assert_eq!(
            OptionBool::from_bytes(&at_bound).unwrap().to_bytes(),
            at_bound.as_slice()
        );
    }

    /// Every retained trailer round-trips, which is what makes `to_bytes`
    /// usable by a builder.
    #[test]
    fn every_retained_trailer_round_trips() {
        for len in 1..=Trailer::MAX {
            let bytes: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(7).wrapping_add(3))
                .collect();
            let v = OptionBool::from_bytes(&bytes).expect("within bound");
            assert_eq!(v.to_bytes(), bytes.as_slice(), "len {len}");
            assert_eq!(OptionBool::from_bytes(v.to_bytes()), Ok(v), "len {len}");
        }
    }

    /// The primitive stays small: it rides inside `Swap` inside `ChainEvent`,
    /// and the ruling after `large_enum_variant` fired two crates up was to
    /// shrink a bottom-crate type rather than allow the lint.
    #[test]
    fn the_type_stays_cheap_and_copy() {
        assert!(
            std::mem::size_of::<OptionBool>() <= 24,
            "OptionBool grew to {} bytes",
            std::mem::size_of::<OptionBool>()
        );
        fn assert_copy<T: Copy>() {}
        assert_copy::<OptionBool>();
    }
}
