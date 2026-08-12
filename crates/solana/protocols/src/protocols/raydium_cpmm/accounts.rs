//! Raydium CPMM account addresses and keys.
//!
//! CPMM pools use a fixed authority PDA and separate token programs
//! per token (supporting Token2022).

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address_with_program_id;

use super::constants::{AUTHORITY, PROGRAM_ID};
use super::state::CpmmPoolState;

/// Keys for a Raydium CPMM pool.
///
/// These are all the addresses needed to build swap instructions.
/// Unlike Raydium V4, CPMM has separate token programs per token
/// (to support Token2022) and no Serum integration.
#[derive(Debug, Clone)]
pub struct RaydiumCpmmKeys {
    /// Pool state account address.
    pub pool_id: Pubkey,

    /// AMM config account.
    pub amm_config: Pubkey,

    /// Token 0 mint.
    pub token_0_mint: Pubkey,

    /// Token 1 mint.
    pub token_1_mint: Pubkey,

    /// Token 0 vault.
    pub token_0_vault: Pubkey,

    /// Token 1 vault.
    pub token_1_vault: Pubkey,

    /// Token 0 program (SPL Token or Token2022).
    pub token_0_program: Pubkey,

    /// Token 1 program.
    pub token_1_program: Pubkey,

    /// Oracle observation state account.
    pub observation_key: Pubkey,
}

impl RaydiumCpmmKeys {
    /// Create keys from a pool state and the pool's on-chain address.
    #[must_use]
    pub fn from_pool_state(pool_id: Pubkey, pool: &CpmmPoolState) -> Self {
        Self {
            pool_id,
            amm_config: pool.amm_config,
            token_0_mint: pool.token_0_mint,
            token_1_mint: pool.token_1_mint,
            token_0_vault: pool.token_0_vault,
            token_1_vault: pool.token_1_vault,
            token_0_program: pool.token_0_program,
            token_1_program: pool.token_1_program,
            observation_key: pool.observation_key,
        }
    }

    /// Get the program ID.
    #[must_use]
    pub fn program_id() -> Pubkey {
        PROGRAM_ID
    }

    /// Get the authority PDA (fixed for all CPMM pools).
    #[must_use]
    pub fn authority() -> Pubkey {
        AUTHORITY
    }

    /// Derive the user's token 0 ATA (uses the correct token program).
    #[must_use]
    pub fn user_token_0_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.token_0_mint,
            &self.token_0_program,
        )
    }

    /// Derive the user's token 1 ATA (uses the correct token program).
    #[must_use]
    pub fn user_token_1_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.token_1_mint,
            &self.token_1_program,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_keys() -> RaydiumCpmmKeys {
        RaydiumCpmmKeys {
            pool_id: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            token_0_mint: Pubkey::new_unique(),
            token_1_mint: Pubkey::new_unique(),
            token_0_vault: Pubkey::new_unique(),
            token_1_vault: Pubkey::new_unique(),
            token_0_program: spl_token::id(),
            token_1_program: spl_token::id(),
            observation_key: Pubkey::new_unique(),
        }
    }

    #[test]
    fn program_id_matches_constant() {
        assert_eq!(RaydiumCpmmKeys::program_id(), PROGRAM_ID);
    }

    #[test]
    fn authority_matches_constant() {
        assert_eq!(RaydiumCpmmKeys::authority(), AUTHORITY);
    }

    #[test]
    fn from_pool_state() {
        let pool_id = Pubkey::new_unique();
        let pool = CpmmPoolState {
            amm_config: Pubkey::new_unique(),
            pool_creator: Pubkey::new_unique(),
            token_0_vault: Pubkey::new_unique(),
            token_1_vault: Pubkey::new_unique(),
            lp_mint: Pubkey::new_unique(),
            token_0_mint: Pubkey::new_unique(),
            token_1_mint: Pubkey::new_unique(),
            token_0_program: spl_token::id(),
            token_1_program: spl_token::id(),
            observation_key: Pubkey::new_unique(),
            auth_bump: 254,
            status: 0,
            lp_mint_decimals: 9,
            mint_0_decimals: 6,
            mint_1_decimals: 9,
            lp_supply: 1_000_000_000,
            protocol_fees_token_0: 0,
            protocol_fees_token_1: 0,
            fund_fees_token_0: 0,
            fund_fees_token_1: 0,
            open_time: 0,
            recent_epoch: 100,
            creator_fee_on: 0,
            enable_creator_fee: false,
            _padding1: [0u8; 6],
            creator_fees_token_0: 0,
            creator_fees_token_1: 0,
            _padding_reserved: [0u64; 28],
        };

        let keys = RaydiumCpmmKeys::from_pool_state(pool_id, &pool);
        assert_eq!(keys.pool_id, pool_id);
        assert_eq!(keys.amm_config, pool.amm_config);
        assert_eq!(keys.token_0_mint, pool.token_0_mint);
        assert_eq!(keys.token_1_mint, pool.token_1_mint);
        assert_eq!(keys.token_0_vault, pool.token_0_vault);
        assert_eq!(keys.token_1_vault, pool.token_1_vault);
    }

    #[test]
    fn user_atas_are_deterministic() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ata0_a = keys.user_token_0_ata(&user);
        let ata0_b = keys.user_token_0_ata(&user);
        assert_eq!(ata0_a, ata0_b);

        let ata1 = keys.user_token_1_ata(&user);
        // Different mints should yield different ATAs
        assert_ne!(ata0_a, ata1);
    }
}
