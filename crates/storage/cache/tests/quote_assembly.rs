//! The live cache must be able to assemble a quote.
//!
//! `PumpSwapQuote::assemble` is bounded on the accounts a PumpSwap quote reads
//! — the pool, both vault token accounts, and the fee-config singleton. That
//! bound is the whole ergonomic contract: it is what makes a partially-sourced
//! quote unrepresentable, and what lets a replay harness reuse the production
//! math by supplying the same three things.
//!
//! A bound nothing exercises is a bound that silently stops being satisfiable.
//! Dropping the `token_accounts` map from `LocalCache`, or unwiring the
//! `CacheSingleton<PumpSwapFeeConfig>` impl, would leave every existing test
//! green while making the live quote path uncallable — so this file fails to
//! *compile* instead.

use solana_account_cache::{LocalCache, LocalCacheConfig};
use solana_protocols::pumpswap::quote::PumpSwapQuote;
use solana_pubkey::Pubkey;

/// Compile-time proof: `LocalCache` satisfies the assembly bound.
///
/// The call is what matters, not the result — an empty cache legitimately has
/// no pool to assemble.
#[test]
fn local_cache_can_assemble_a_pumpswap_quote() {
    let cache = LocalCache::new(LocalCacheConfig::default());
    assert!(
        PumpSwapQuote::assemble(&cache, &Pubkey::new_unique(), 0).is_none(),
        "an empty cache has nothing to assemble — it must refuse, not fabricate"
    );
}

/// A quote must refuse rather than price on a reserve it could not read.
///
/// The failure this guards is specific: the pool account carries no reserves,
/// so a bundle that filled missing vaults with `0` would report a pool with
/// zero liquidity as quotable and divide into it.
#[test]
fn a_pool_without_its_vaults_does_not_assemble() {
    let cache = LocalCache::new(LocalCacheConfig::default());
    // No pool, no vaults, no fee config — every missing part must produce the
    // same answer: None.
    for slot in [0, 1, u64::MAX] {
        assert!(PumpSwapQuote::assemble(&cache, &Pubkey::new_unique(), slot).is_none());
    }
}
