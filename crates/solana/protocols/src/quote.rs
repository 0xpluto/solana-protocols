//! `QuoteState` — one enum over every protocol that can price a swap.
//!
//! Each variant holds a *quote bundle* (the accounts a quote reads, assembled),
//! not a single pool account. That is the difference from [`PoolState`], which
//! this replaces: a `PoolState::PumpSwap` is a pool account that cannot answer
//! what a swap costs, because PumpSwap keeps its reserves in vault accounts and
//! its fee rates in a config singleton.
//!
//! # The table is the source of truth
//!
//! `quote_protocols!` takes one row per protocol and generates the enum, the
//! cache bound, and every dispatch arm. Adding a protocol is a row.
//!
//! It also names the protocols that have **not** been ported, which is what
//! keeps the dispatch exhaustive. A `_ => None` arm would compile forever and
//! silently swallow a newly-added `Protocol` variant; naming them means adding
//! one breaks this file, which is the whole point of the no-wildcard rule.
//!
//! # Scope
//!
//! Pumpfun and PumpSwap are ported: one token lifecycle (bonding curve →
//! graduated AMM) and ~85% of observed swaps. The rest are deliberately parked
//! — see the `not_ported` list, which is that decision written down rather than
//! implied by an absence.
//!
//! [`PoolState`]: crate::protocols::PoolState

use crate::events::SwapOutput;
use crate::protocols::pumpfun::quote::{PumpfunQuote, PumpfunQuoteCache};
use crate::protocols::pumpswap::quote::{PumpSwapQuote, PumpSwapQuoteCache};
use crate::protocols::Protocol;
use crate::traits::{SwapMath, SwapParams};
use solana_program::pubkey::Pubkey;

/// Why a pool could not be quoted.
///
/// The two cases are deliberately distinct. "We never wrote a bundle for this
/// protocol" is a permanent, structural fact about our code; "the accounts are
/// not in the cache yet" is transient and resolves on its own. Collapsing them
/// into one `None` would make a missing port indistinguishable from a cold
/// cache — and the operator response to each is opposite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotQuotable {
    /// No quote bundle exists for this protocol yet.
    #[error("{0:?} has no quote bundle — protocol not ported")]
    NotPorted(Protocol),

    /// The bundle exists, but at least one account it reads was absent from
    /// the cache at the requested slot.
    #[error("{0:?} quote state incomplete at this slot — an account it reads is not cached")]
    Incomplete(Protocol),
}

macro_rules! quote_protocols {
    (
        ported { $( $variant:ident => $bundle:ty as $bound:ident ),+ $(,)? }
        not_ported { $( $unported:ident ),* $(,)? }
    ) => {
        /// A quote bundle for whichever protocol owns the pool.
        ///
        /// Variants are boxed so the enum stays two words regardless of how
        /// many accounts any one bundle grows to read — a PumpSwap quote
        /// already carries a pool, two token accounts and a fee config, and
        /// an unboxed enum would make every `QuoteState` as large as the
        /// widest protocol. Boxing uniformly rather than only the current
        /// largest avoids an asymmetry that flips the next time a bundle
        /// gains a field.
        #[derive(Debug, Clone)]
        pub enum QuoteState {
            $( #[doc = concat!(stringify!($variant), " quote bundle.")]
               $variant(::std::boxed::Box<$bundle>), )+
        }

        /// A cache able to supply every account any ported quote reads.
        ///
        /// The union of the per-bundle cache traits `#[derive(QuoteState)]`
        /// emits, composed by name — this layer never learns any protocol's
        /// fields, so adding an account to a bundle widens this bound
        /// automatically.
        pub trait QuoteCache: $( $bound + )+ {}
        impl<T> QuoteCache for T where T: $( $bound + )+ {}

        impl QuoteState {
            /// Assemble the bundle for `protocol` at `slot`.
            ///
            /// # Errors
            ///
            /// [`NotQuotable::NotPorted`] when no bundle exists for the
            /// protocol; [`NotQuotable::Incomplete`] when one does but an
            /// account it reads is not cached at `slot`.
            pub fn assemble<C>(
                protocol: Protocol,
                cache: &C,
                pool: &Pubkey,
                slot: u64,
            ) -> Result<Self, NotQuotable>
            where
                C: QuoteCache + ?Sized,
            {
                match protocol {
                    $( Protocol::$variant => <$bundle>::assemble(cache, pool, slot)
                        .map(|q| Self::$variant(::std::boxed::Box::new(q)))
                        .ok_or(NotQuotable::Incomplete(protocol)), )+
                    $( Protocol::$unported => Err(NotQuotable::NotPorted(protocol)), )*
                }
            }

            /// Which protocol this bundle prices.
            #[must_use]
            pub const fn protocol(&self) -> Protocol {
                match self {
                    $( Self::$variant(_) => Protocol::$variant, )+
                }
            }
        }

        impl SwapMath for QuoteState {
            fn calculate_swap(&self, params: &SwapParams) -> crate::Result<SwapOutput> {
                match self {
                    $( Self::$variant(q) => q.calculate_swap(params), )+
                }
            }

            fn spot_price(&self) -> f64 {
                match self {
                    $( Self::$variant(q) => q.spot_price(), )+
                }
            }

            fn is_active(&self) -> bool {
                match self {
                    $( Self::$variant(q) => q.is_active(), )+
                }
            }
        }
    };
}

quote_protocols! {
    ported {
        Pumpfun => PumpfunQuote as PumpfunQuoteCache,
        PumpSwap => PumpSwapQuote as PumpSwapQuoteCache,
    }
    not_ported {
        RaydiumV4,
        RaydiumClmm,
        RaydiumCpmm,
        RaydiumLaunchpad,
        MeteoraDlmm,
        MeteoraDbC,
        MeteoraDammV2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enum stays pointer-sized as bundles grow.
    ///
    /// Pinned rather than assumed: this is what stops a `QuoteState` in a
    /// collection from costing the widest protocol's footprint per element,
    /// and it is exactly the property an un-boxed variant would silently
    /// remove.
    #[test]
    fn quote_state_stays_two_words() {
        assert_eq!(
            std::mem::size_of::<QuoteState>(),
            2 * std::mem::size_of::<usize>()
        );
    }

    /// An un-ported protocol is refused as *un-ported*, not as a cache miss.
    ///
    /// These are the two failures a single `Option` would have merged. One is
    /// permanent and means "write a bundle"; the other is transient and means
    /// "wait for the account". Answering the wrong one sends an operator
    /// looking in the wrong place.
    #[test]
    fn an_unported_protocol_says_so_rather_than_looking_like_a_cache_miss() {
        struct EmptyCache;
        // Every `CacheGet`/`CacheSingleton` the bound needs, all answering
        // "not cached" — so a ported protocol here yields Incomplete.
        impl<V> solana_account_traits::CacheGet<Pubkey, V> for EmptyCache {
            fn get(&self, _: &Pubkey) -> Option<V> {
                None
            }
            fn get_with_slot(&self, _: &Pubkey) -> Option<(V, u64)> {
                None
            }
            fn get_at_slot(&self, _: &Pubkey, _: u64) -> Option<V> {
                None
            }
            fn get_at_slot_with_slot(&self, _: &Pubkey, _: u64) -> Option<(V, u64)> {
                None
            }
        }
        impl<V> solana_account_traits::CacheSingleton<V> for EmptyCache {
            fn get(&self) -> Option<V> {
                None
            }
            fn set(&self, _: V) {}
        }

        let pool = Pubkey::new_unique();
        assert_eq!(
            QuoteState::assemble(Protocol::RaydiumV4, &EmptyCache, &pool, 0).unwrap_err(),
            NotQuotable::NotPorted(Protocol::RaydiumV4),
        );
        assert_eq!(
            QuoteState::assemble(Protocol::PumpSwap, &EmptyCache, &pool, 0).unwrap_err(),
            NotQuotable::Incomplete(Protocol::PumpSwap),
        );
    }
}
