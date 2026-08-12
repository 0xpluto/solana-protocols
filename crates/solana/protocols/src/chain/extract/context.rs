//! Narrow view of transaction context the extractor needs.
//!
//! Pumpfun-style protocols (log-only) don't need any of this —
//! [`NoContext`] satisfies the trait trivially. AMM-style protocols
//! (PumpSwap, Raydium, Meteora) need token-account → mint lookup +
//! pre/post balances to resolve CPI transfer amounts.
//!
//! The real implementation lives in the `tx-context` crate, which
//! impls this trait on its `TransactionContext`. Keeping the trait
//! here means `solana-protocols` doesn't pull in the cache-adapter
//! dependency chain.
//!
//! *Used via dyn dispatch* in the registry so the extractor map can be
//! type-erased over one uniform function signature.

use solana_program::pubkey::Pubkey;

/// Fields the extractor needs to peek out of a transaction beyond
/// what's in the [`ParsedInstruction`](crate::parsing::ParsedInstruction)s
/// themselves.
pub trait ExtractContext {
    /// Mint of a given token account address, if known.
    fn token_mint(&self, token_account: &Pubkey) -> Option<Pubkey>;

    /// Pre-transaction balance for a token account (smallest units).
    fn pre_balance(&self, token_account: &Pubkey) -> Option<u64>;

    /// Post-transaction balance for a token account (smallest units).
    fn post_balance(&self, token_account: &Pubkey) -> Option<u64>;

    /// Balance delta (post − pre). Useful when resolving CPI transfer
    /// amounts without walking the inner instructions.
    fn balance_delta(&self, token_account: &Pubkey) -> Option<i128> {
        let pre = self.pre_balance(token_account)? as i128;
        let post = self.post_balance(token_account)? as i128;
        Some(post - pre)
    }
}

/// Zero-cost [`ExtractContext`] that always reports "don't know".
///
/// Use when the tx source doesn't carry balance data, or when you're
/// calling an extractor you know is log-only (Pumpfun, bonding-curve
/// protocols). AMM extractors will return `None` through it and their
/// events will be skipped.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoContext;

impl ExtractContext for NoContext {
    fn token_mint(&self, _: &Pubkey) -> Option<Pubkey> {
        None
    }
    fn pre_balance(&self, _: &Pubkey) -> Option<u64> {
        None
    }
    fn post_balance(&self, _: &Pubkey) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_context_returns_none_for_every_lookup() {
        let ctx = NoContext;
        let key = Pubkey::new_unique();
        assert!(ctx.token_mint(&key).is_none());
        assert!(ctx.pre_balance(&key).is_none());
        assert!(ctx.post_balance(&key).is_none());
        assert!(ctx.balance_delta(&key).is_none());
    }
}
