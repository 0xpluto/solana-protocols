//! Layouts that name both sides of a pool's pair.
//!
//! Route discovery needs four facts per pool: the pool account, whose math
//! applies, and the two tokens. This module carries the two that depend on a
//! layout.
//!
//! # Only complete answers
//!
//! [`NamesPair`] is implemented **only** where both mints are actually in the
//! layout. A layout that cannot answer does not implement it -- there is no
//! partial variant and no `Option`, because a pair that is sometimes present
//! is exactly how a routing graph goes silently wrong: a `None` reads as "no
//! edge here" from every call site, which is indistinguishable from "no pool
//! here".
//!
//! Layouts that hold one side, or none, are a separate problem with a separate
//! answer -- a second account read -- and they will get a type that says so
//! rather than being folded in here as absence.
//!
//! # Why two traits
//!
//! An instruction's accounts name the pool, because the pool is one of the
//! accounts. A deserialized pool *state* cannot: it is the account, and it has
//! no idea what pubkey it was read from -- the caller holds that. So `pool()`
//! lives on [`SwapAccounts`], and the pair, which both can answer, lives on
//! the trait they share.

use solana_program::pubkey::Pubkey;

use crate::parsing::FromAccountKeys;

/// An on-chain layout that names both tokens of a pool.
///
/// The order returned is whatever the layout uses; callers that need a stable
/// identity should build a [`PoolEdge`](crate::chain::PoolEdge), which orders
/// the pair canonically.
pub trait NamesPair {
    /// The two tokens this pool joins.
    fn pair(&self) -> (Pubkey, Pubkey);
}

/// A swap instruction's accounts struct, which also names the pool.
///
/// Implementing this is what makes an instruction discoverable: given the
/// account list a program actually accepted, all four edge facts fall out
/// without decoding the instruction body, the event, or the logs.
pub trait SwapAccounts: NamesPair + FromAccountKeys {
    /// The account holding reserves -- AMM pool, bonding curve, or CLMM pool.
    fn pool(&self) -> Pubkey;

    /// Read the pool and its pair straight from an account list.
    ///
    /// Returns `None` when the list is not one this layout can name. That is
    /// deliberate at this level: the caller knows which instruction it was
    /// resolving and can report the refusal against it, which a bare `None`
    /// here could not.
    fn read(keys: &[Pubkey]) -> Option<(Pubkey, (Pubkey, Pubkey))> {
        let a = Self::from_account_keys(keys).ok()?;
        Some((a.pool(), a.pair()))
    }
}
