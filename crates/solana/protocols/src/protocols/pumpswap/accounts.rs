//! PumpSwap account derivations and key management.
//!
//! Handles PDA derivation for all PumpSwap accounts and provides
//! the [`PumpSwapKeys`] struct for instruction building.

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;

use super::constants::{CREATOR_VAULT_SEED, FEE_PROGRAM, PROGRAM_ID, USER_VOLUME_ACCUMULATOR_SEED};

/// All keys needed to build PumpSwap swap instructions.
///
/// PumpSwap pool addresses are NOT PDAs — they come from pool creation
/// events or gRPC account subscriptions. This struct stores the pool
/// address and all derived accounts needed for swap instructions.
///
/// # Construction
///
/// Use [`PumpSwapKeys::new`] when you know the pool address and coin creator,
/// or [`PumpSwapKeys::from_pool_state`] to extract keys from a parsed pool.
#[derive(Debug, Clone)]
pub struct PumpSwapKeys {
    /// Pool account address.
    pub pool: Pubkey,
    /// Base token mint (meme token).
    pub base_mint: Pubkey,
    /// Quote token mint (typically WSOL).
    pub quote_mint: Pubkey,
    /// Pool's base token vault account.
    pub pool_base_token_account: Pubkey,
    /// Pool's quote token vault account.
    pub pool_quote_token_account: Pubkey,
    /// Original coin creator.
    pub coin_creator: Pubkey,
    /// Creator vault authority PDA (derived from creator + CREATOR_VAULT_SEED).
    pub creator_vault_authority: Pubkey,
    /// Creator vault ATA (WSOL ATA of creator_vault_authority).
    pub creator_vault_ata: Pubkey,
}

impl PumpSwapKeys {
    /// Create keys from pool address and pool state fields.
    ///
    /// Derives creator vault authority PDA and ATA automatically.
    #[must_use]
    pub fn new(
        pool: Pubkey,
        base_mint: Pubkey,
        quote_mint: Pubkey,
        pool_base_token_account: Pubkey,
        pool_quote_token_account: Pubkey,
        coin_creator: Pubkey,
    ) -> Self {
        let creator_vault_authority = derive_creator_vault_authority(&coin_creator);
        let creator_vault_ata = get_associated_token_address(&creator_vault_authority, &quote_mint);

        PumpSwapKeys {
            pool,
            base_mint,
            quote_mint,
            pool_base_token_account,
            pool_quote_token_account,
            coin_creator,
            creator_vault_authority,
            creator_vault_ata,
        }
    }

    /// Create keys from a parsed PumpSwap pool state.
    ///
    /// The pool address must be provided separately since it's not
    /// stored in the pool state data itself.
    #[must_use]
    pub fn from_pool_state(pool_address: Pubkey, pool: &super::PumpSwapPool) -> Self {
        Self::new(
            pool_address,
            pool.base_mint,
            pool.quote_mint,
            pool.pool_base_token_account,
            pool.pool_quote_token_account,
            pool.coin_creator,
        )
    }

    /// Get the user's base token ATA (meme token).
    #[must_use]
    pub fn user_base_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address(user, &self.base_mint)
    }

    /// Get the user's quote token ATA (WSOL).
    #[must_use]
    pub fn user_quote_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address(user, &self.quote_mint)
    }

    /// Get the user's volume accumulator PDA.
    #[must_use]
    pub fn user_volume_accumulator(&self, user: &Pubkey) -> Pubkey {
        derive_user_volume_accumulator(user)
    }
}

/// Derive the creator vault authority PDA.
///
/// This PDA controls the creator vault ATA that receives creator fees.
/// Seeds: `["creator_vault", coin_creator]` under PumpSwap program.
#[must_use]
pub fn derive_creator_vault_authority(coin_creator: &Pubkey) -> Pubkey {
    let (pda, _bump) =
        Pubkey::find_program_address(&[CREATOR_VAULT_SEED, coin_creator.as_ref()], &PROGRAM_ID);
    pda
}

/// Derive the user volume accumulator PDA.
///
/// Tracks cumulative volume for fee discounts.
/// Seeds: `["user_volume_accumulator", user]` under fee program.
#[must_use]
pub fn derive_user_volume_accumulator(user: &Pubkey) -> Pubkey {
    let (pda, _bump) =
        Pubkey::find_program_address(&[USER_VOLUME_ACCUMULATOR_SEED, user.as_ref()], &FEE_PROGRAM);
    pda
}

#[cfg(test)]
mod tests {
    use super::super::PumpSwapPool;
    use super::*;

    #[test]
    fn pda_derivation_deterministic() {
        let creator = Pubkey::new_unique();

        let pda1 = derive_creator_vault_authority(&creator);
        let pda2 = derive_creator_vault_authority(&creator);
        assert_eq!(pda1, pda2);
    }

    #[test]
    fn user_volume_accumulator_deterministic() {
        let user = Pubkey::new_unique();

        let pda1 = derive_user_volume_accumulator(&user);
        let pda2 = derive_user_volume_accumulator(&user);
        assert_eq!(pda1, pda2);
    }

    #[test]
    fn keys_derives_all() {
        let pool = Pubkey::new_unique();
        let base_mint = Pubkey::new_unique();
        let quote_mint = Pubkey::new_unique();
        let pool_base = Pubkey::new_unique();
        let pool_quote = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let user = Pubkey::new_unique();

        let keys = PumpSwapKeys::new(pool, base_mint, quote_mint, pool_base, pool_quote, creator);

        assert_eq!(keys.pool, pool);
        assert_eq!(keys.base_mint, base_mint);
        assert_eq!(keys.quote_mint, quote_mint);
        assert_eq!(keys.pool_base_token_account, pool_base);
        assert_eq!(keys.pool_quote_token_account, pool_quote);
        assert_eq!(keys.coin_creator, creator);
        assert_eq!(
            keys.creator_vault_authority,
            derive_creator_vault_authority(&creator)
        );
        assert_eq!(
            keys.creator_vault_ata,
            get_associated_token_address(&keys.creator_vault_authority, &quote_mint)
        );

        // User-specific derivations
        assert_eq!(
            keys.user_base_ata(&user),
            get_associated_token_address(&user, &base_mint)
        );
        assert_eq!(
            keys.user_quote_ata(&user),
            get_associated_token_address(&user, &quote_mint)
        );
    }

    #[test]
    fn keys_from_pool_state() {
        let pool_address = Pubkey::new_unique();
        let pool = PumpSwapPool {
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            pool_base_token_account: Pubkey::new_unique(),
            pool_quote_token_account: Pubkey::new_unique(),
            coin_creator: Pubkey::new_unique(),
            ..PumpSwapPool::default()
        };

        let keys = PumpSwapKeys::from_pool_state(pool_address, &pool);

        assert_eq!(keys.pool, pool_address);
        assert_eq!(keys.base_mint, pool.base_mint);
        assert_eq!(keys.quote_mint, pool.quote_mint);
        assert_eq!(keys.pool_base_token_account, pool.pool_base_token_account);
        assert_eq!(keys.pool_quote_token_account, pool.pool_quote_token_account);
        assert_eq!(keys.coin_creator, pool.coin_creator);
    }
}
