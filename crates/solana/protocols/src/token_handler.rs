//! Cache handler for SPL / Token-2022 **token accounts**.
//!
//! This is what makes AMM reserves live. PumpSwap (and Raydium, and DAMM) hold
//! their reserves in vault *token accounts*, not inline in the pool account — so
//! a pool update alone never tells you the pool's balances. The pool handler
//! reports its two vaults as dependencies; the ingest layer subscribes them; and
//! this handler decodes the resulting account updates into the cache, where the
//! quoter reads `pool -> vault pubkeys -> balances`.
//!
//! Feeding reserves from the *account stream* rather than from parsed swap
//! transactions is deliberate: the account stream reports **every** balance
//! change regardless of which instruction caused it, so a liquidity deposit, an
//! unparsed instruction variant, or a plain transfer all keep the cache honest.
//! A transaction-parsing feed only updates on the instruction kinds it happens
//! to decode, and silently stales on everything else.
//!
//! Registration is **per token program** — the registry keys on the account's
//! owner, and a token account is owned by whichever token program minted it:
//!
//! ```ignore
//! registry.register(TokenAccountHandler::new(TokenProgram::SplToken));
//! registry.register(TokenAccountHandler::new(TokenProgram::SplToken2022));
//! ```
//!
//! Only compiled when the `cache-handlers` feature is enabled.

use solana_program::pubkey::Pubkey;
use solana_account_traits::{
    CacheInsert, HandleResult, HandlerError, ProtocolStateHandler, StorageHandler,
};

use crate::tokens::{TokenAccount, TokenProgram, TOKEN_ACCOUNT_BASE_LEN};

/// Decodes token accounts (vault balances, user ATAs) into the cache.
///
/// Non-Anchor: token accounts carry no discriminator, so this handler takes the
/// registry's fallback path with a **size predicate** rather than a prefix
/// match. `matches_account` is therefore load-bearing — without it the handler
/// would claim every account its program owns (mints included, which are 82
/// bytes and would decode into nonsense).
#[derive(Debug, Clone, Copy)]
pub struct TokenAccountHandler {
    program: TokenProgram,
}

impl TokenAccountHandler {
    /// Handler for accounts owned by `program`.
    #[must_use]
    pub const fn new(program: TokenProgram) -> Self {
        Self { program }
    }
}

impl ProtocolStateHandler for TokenAccountHandler {
    type State = TokenAccount;

    fn program_id(&self) -> Pubkey {
        self.program.id()
    }

    /// Token accounts have no discriminator — dispatch is by owner + the
    /// size predicate below.
    fn discriminator(&self) -> Option<&'static [u8]> {
        None
    }

    /// Claim only accounts large enough to be token accounts. Mints (82 bytes)
    /// and other program-owned accounts fall through; Token-2022 accounts with
    /// TLV extensions are longer than the base and are claimed correctly.
    fn matches_account(&self, data: &[u8]) -> bool {
        data.len() >= TOKEN_ACCOUNT_BASE_LEN
    }

    /// **Never** subscribe wholesale. This handler is registered under the SPL
    /// Token program; "every account this program owns" is most of Solana. The
    /// vaults it decodes arrive because a pool handler named them in
    /// `accounts_to_subscribe`, i.e. one pubkey at a time.
    fn subscribe_program_accounts(&self) -> bool {
        false
    }

    fn deserialize(&self, data: &[u8]) -> Result<Self::State, HandlerError> {
        TokenAccount::from_account_data(data, self.program).map_err(|e| HandlerError::Deserialize {
            data_len: data.len(),
            reason: e.to_string(),
        })
    }
}

impl<C> StorageHandler<C> for TokenAccountHandler
where
    C: CacheInsert<Pubkey, TokenAccount> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        cache.insert(*pubkey, state.clone(), slot);
        Ok(HandleResult::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real PumpSwap quote vault decodes to the mint/owner/balance the chain
    /// reports. Fixture-pinned per the decoder-verification suite: the layout
    /// claim is checked against bytes, not against a doc comment.
    #[test]
    fn decodes_onchain_vault_token_account() {
        let fx = crate::test_fixtures::AccountFixture::load("spl_token/pumpswap_quote_vault.json");
        let acct = TokenAccount::from_account_data(fx.data(), TokenProgram::SplToken)
            .expect("real vault decodes");
        assert_eq!(acct.token.mint.to_string(), fx.expected_str("mint"));
        assert_eq!(acct.owner.to_string(), fx.expected_str("owner"));
        assert_eq!(
            acct.balance,
            u64::try_from(fx.expected_i64("amount")).expect("balance fits"),
        );
    }

    /// Registering this handler must not widen a wholesale subscription — the
    /// filter comes from `subscribable_program_ids`, and SPL Token must never
    /// appear there (that would subscribe to most of Solana).
    #[test]
    fn token_program_is_never_subscribed_wholesale() {
        use solana_account_traits::{HandlerRegistry, InsertOutcome};
        struct NoCache;
        impl CacheInsert<Pubkey, TokenAccount> for NoCache {
            fn insert(&self, _: Pubkey, _: TokenAccount, _: u64) -> InsertOutcome {
                InsertOutcome::Inserted
            }
        }
        let mut reg: HandlerRegistry<NoCache> = HandlerRegistry::new();
        reg.register(TokenAccountHandler::new(TokenProgram::SplToken));
        reg.register(TokenAccountHandler::new(TokenProgram::SplToken2022));

        assert_eq!(reg.subscribable_program_ids().count(), 0);
        assert_eq!(reg.program_ids().count(), 2, "still dispatchable");
    }

    #[test]
    fn rejects_data_shorter_than_the_base_layout() {
        assert!(TokenAccount::from_account_data(&[0u8; 100], TokenProgram::SplToken).is_err());
    }

    /// The size predicate is the whole gate for a discriminator-less handler:
    /// an 82-byte mint account must not be claimed as a token account.
    #[test]
    fn size_predicate_rejects_mint_accounts() {
        let handler = TokenAccountHandler::new(TokenProgram::SplToken);
        assert!(!handler.matches_account(&[0u8; 82]));
        assert!(handler.matches_account(&[0u8; TOKEN_ACCOUNT_BASE_LEN]));
        // Token-2022 with extensions: longer than base, still ours.
        assert!(handler.matches_account(&[0u8; 256]));
    }
}
