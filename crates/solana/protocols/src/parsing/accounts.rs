//! What an account slot *is* — the four kinds, and why they are four.
//!
//! An instruction is sent a `Vec<Pubkey>`: no names, no gaps, no metadata. Every
//! name we attach comes from the program's IDL, and the IDL declares only the
//! slots the program always reads. Real instructions carry more. Deciding what
//! "more" means turns out to have four distinct answers with four different wire
//! encodings, and conflating any two of them produces a wrong pubkey that looks
//! like a right one.
//!
//! # The four kinds
//!
//! | Kind | Declared in IDL | When absent | Index |
//! |---|---|---|---|
//! | **Required** | yes | cannot be | fixed |
//! | **Optional** (`optional: true`) | yes | slot holds the **program id** | fixed |
//! | **Conditional** | no | **slot does not exist** | after the declared list |
//! | **Rest** | no | empty | last |
//!
//! ## Required and Optional both occupy an index. Conditional does not.
//!
//! This is the distinction worth having a type for. Anchor's optional accounts
//! keep their slot and put the program's own id in it as a sentinel — so an
//! instruction with three optional accounts, all absent, still sends all three
//! slots. A conditional account is the opposite: absent means the slot is not
//! there, and everything after it shifts down.
//!
//! Both would be `Option<Pubkey>` in Rust. They are not the same fact, and a
//! single type for both would encode one of them wrongly on the way out. So
//! [`Conditional`] exists rather than reusing `Option` — for the same reason
//! [`Legacy`](crate::parsing::state::Legacy) exists rather than reusing `Option`
//! for version-added struct fields: `Option`'s combinators are the ergonomic
//! path to collapsing two facts that must not collapse.
//!
//! ## Conditionals are a prefix, not a set
//!
//! Because an absent conditional does not occupy a slot, `absent` followed by
//! `present` is unrepresentable on the wire: the second account would simply be
//! at the first one's index, and nothing distinguishes them. So the conditionals
//! of an instruction form a **prefix** — the k-th is present only if all k-1
//! before it are.
//!
//! The parse direction cannot violate this: it fills conditionals from the
//! actual account count, in order, so a hole cannot be produced. The build
//! direction can, because a caller sets the fields — so it is checked there, and
//! [`RemainingSequence`] is what it reports.
//!
//! ## Rest is last, and only reachable behind a full prefix
//!
//! A rest is a homogeneous list — CLMM's `swap_v2` sends a tick-array bitmap
//! extension and then some number of tick arrays. It stays a `Vec<Pubkey>`
//! rather than an enum because 0..N homogeneous accounts *is* a `Vec`; the thing
//! an enum was wanted for is the sequencing rule, and that lives on
//! [`Conditional`] plus the check below. Its entries begin only after every
//! conditional is present — otherwise a rest entry would sit at an absent
//! conditional's index and be read as that conditional.

use solana_program::pubkey::Pubkey;

/// An account past the IDL's declared list: present, or its slot does not exist.
///
/// Deliberately not `Option<Pubkey>`. An `Option` here would be
/// indistinguishable from an Anchor `optional: true` account, which occupies its
/// slot when absent by holding the program's id — the opposite wire encoding.
/// See the [module docs](self) for the full table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Conditional {
    /// The instruction did not send this account, so the slot is not there.
    #[default]
    Absent,
    /// The instruction sent it.
    Present(Pubkey),
}

impl Conditional {
    /// Whether the slot exists.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }

    /// The pubkey, if the slot exists.
    ///
    /// Named `key` rather than given a `From`/`Into` to `Option` on purpose: the
    /// conversion is where the two encodings would start being treated as one.
    #[must_use]
    pub const fn key(&self) -> Option<Pubkey> {
        match self {
            Self::Present(k) => Some(*k),
            Self::Absent => None,
        }
    }
}

/// A conditional account is present while an earlier one is absent.
///
/// Unrepresentable on the wire — the later account would occupy the earlier
/// one's index — so building such an instruction is refused rather than sent.
/// The parse direction cannot produce this, which is why it is an error type for
/// the build path only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "`{present}` is present but `{absent}` before it is not: conditional accounts \
     are a prefix, so `{present}` would be sent at `{absent}`'s index"
)]
pub struct RemainingSequence {
    /// The absent account that leaves a hole.
    pub absent: &'static str,
    /// The first present account after it.
    pub present: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Conditional::default()` must be `Absent`: a defaulted account slot that
    /// claimed to hold a pubkey would hold `Pubkey::default()`, which is a real
    /// address (the system program) and would be sent.
    #[test]
    fn the_default_is_absent() {
        assert_eq!(Conditional::default(), Conditional::Absent);
        assert!(!Conditional::default().is_present());
        assert_eq!(Conditional::default().key(), None);
    }

    /// The error has to name both sides, or a builder failure says a rule was
    /// broken without saying which two accounts broke it.
    #[test]
    fn the_sequence_error_names_both_accounts() {
        let e = RemainingSequence {
            absent: "bitmap_extension",
            present: "tick_array",
        };
        let msg = e.to_string();
        assert!(
            msg.contains("bitmap_extension") && msg.contains("tick_array"),
            "{msg}"
        );
    }
}
