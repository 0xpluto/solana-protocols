//! The live cache must satisfy the *composed* quote bound.
//!
//! `QuoteCache` is the union of every ported protocol's per-bundle cache
//! trait. Each of those is generated from that bundle's field list, so adding
//! an account to any quote widens this bound automatically — and `LocalCache`
//! must keep up. A bound nothing exercises is one that silently stops being
//! satisfiable, so this fails to *compile* rather than failing quietly at
//! runtime when a pool turns out to be unquotable.

use solana_account_cache::{LocalCache, LocalCacheConfig};
use solana_protocols::protocols::Protocol;
use solana_protocols::quote::{NotQuotable, QuoteState};
use solana_pubkey::Pubkey;

/// Compile-time proof: `LocalCache` can assemble every ported protocol.
#[test]
fn local_cache_satisfies_the_composed_quote_bound() {
    let cache = LocalCache::new(LocalCacheConfig::default());
    let pool = Pubkey::new_unique();

    for protocol in [Protocol::Pumpfun, Protocol::PumpSwap] {
        // An empty cache legitimately has nothing to assemble — what matters
        // is that the call type-checks, and that the refusal names the right
        // reason.
        assert_eq!(
            QuoteState::assemble(protocol, &cache, &pool, 0).unwrap_err(),
            NotQuotable::Incomplete(protocol),
            "a ported protocol on a cold cache is Incomplete, never NotPorted"
        );
    }
}

/// A parked protocol reports itself parked, through the real cache.
///
/// Pairs with the unit test in `solana-protocols`: that one proves the
/// dispatch distinguishes the cases, this one proves the distinction survives
/// the actual cache type an operator would hit.
#[test]
fn a_parked_protocol_is_reported_as_unported_not_as_missing_data() {
    let cache = LocalCache::new(LocalCacheConfig::default());
    assert_eq!(
        QuoteState::assemble(Protocol::MeteoraDlmm, &cache, &Pubkey::new_unique(), 0).unwrap_err(),
        NotQuotable::NotPorted(Protocol::MeteoraDlmm),
    );
}
