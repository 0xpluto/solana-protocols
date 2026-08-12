//! Raydium Launchpad account addresses and keys.
//!
//! Launchpad pools use configurable token programs (SPL/Token2022)
//! and Anchor's event authority PDA pattern.

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address_with_program_id;

use super::constants::PROGRAM_ID;
use super::state::LaunchpadPoolState;

/// Keys for a Raydium Launchpad pool.
///
/// These are all the addresses needed to build swap instructions.
/// Launchpad has 15 accounts including event_authority and program self-ref.
#[derive(Debug, Clone)]
pub struct LaunchpadKeys {
    /// Pool state account address.
    pub pool_id: Pubkey,

    /// Auth PDA (vault authority).
    pub authority: Pubkey,

    /// Global config account.
    pub global_config: Pubkey,

    /// Platform config account.
    pub platform_config: Pubkey,

    /// Base token mint (launchpad token).
    pub base_mint: Pubkey,

    /// Quote token mint (usually SOL).
    pub quote_mint: Pubkey,

    /// Base token vault.
    pub base_vault: Pubkey,

    /// Quote token vault.
    pub quote_vault: Pubkey,

    /// Base token program (SPL Token or Token2022).
    pub base_token_program: Pubkey,

    /// Quote token program.
    pub quote_token_program: Pubkey,
}

impl LaunchpadKeys {
    /// Create keys from a pool state and the pool's on-chain address.
    ///
    /// Derives the authority PDA and resolves token programs from the
    /// pool's `token_program_flag`.
    #[must_use]
    pub fn from_pool_state(pool_id: Pubkey, pool: &LaunchpadPoolState) -> Self {
        let authority = derive_authority(&pool_id, pool.auth_bump);
        let (base_token_program, quote_token_program) =
            resolve_token_programs(pool.token_program_flag);

        Self {
            pool_id,
            authority,
            global_config: pool.global_config,
            platform_config: pool.platform_config,
            base_mint: pool.base_mint,
            quote_mint: pool.quote_mint,
            base_vault: pool.base_vault,
            quote_vault: pool.quote_vault,
            base_token_program,
            quote_token_program,
        }
    }

    /// Get the program ID.
    #[must_use]
    pub fn program_id() -> Pubkey {
        PROGRAM_ID
    }

    /// Derive the event authority PDA (Anchor standard).
    #[must_use]
    pub fn event_authority() -> Pubkey {
        Pubkey::find_program_address(&[b"__event_authority"], &PROGRAM_ID).0
    }

    /// Derive the user's base token ATA.
    #[must_use]
    pub fn user_base_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.base_mint,
            &self.base_token_program,
        )
    }

    /// Derive the user's quote token ATA.
    #[must_use]
    pub fn user_quote_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.quote_mint,
            &self.quote_token_program,
        )
    }
}

/// Derive the vault authority PDA for a specific pool.
///
/// Seeds: `["vault_and_lp_mint_auth_seed", pool_id]` with bump.
#[must_use]
pub fn derive_authority(pool_id: &Pubkey, bump: u8) -> Pubkey {
    Pubkey::create_program_address(
        &[b"vault_and_lp_mint_auth_seed", pool_id.as_ref(), &[bump]],
        &PROGRAM_ID,
    )
    .unwrap_or_default()
}

/// Resolve token programs from the pool's `token_program_flag`.
///
/// bit0 = base token (0=SPL, 1=Token2022)
/// bit1 = quote token (0=SPL, 1=Token2022)
#[must_use]
pub fn resolve_token_programs(flag: u8) -> (Pubkey, Pubkey) {
    let base_program = if flag & 1 == 0 {
        spl_token::id()
    } else {
        spl_token_2022::id()
    };
    let quote_program = if flag & 2 == 0 {
        spl_token::id()
    } else {
        spl_token_2022::id()
    };
    (base_program, quote_program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_id_matches_constant() {
        assert_eq!(LaunchpadKeys::program_id(), PROGRAM_ID);
    }

    #[test]
    fn event_authority_is_deterministic() {
        let ea1 = LaunchpadKeys::event_authority();
        let ea2 = LaunchpadKeys::event_authority();
        assert_eq!(ea1, ea2);
        assert_ne!(ea1, Pubkey::default());
    }

    #[test]
    fn resolve_token_programs_spl_only() {
        let (base, quote) = resolve_token_programs(0);
        assert_eq!(base, spl_token::id());
        assert_eq!(quote, spl_token::id());
    }

    #[test]
    fn resolve_token_programs_base_2022() {
        let (base, quote) = resolve_token_programs(1); // bit0=1
        assert_eq!(base, spl_token_2022::id());
        assert_eq!(quote, spl_token::id());
    }

    #[test]
    fn resolve_token_programs_both_2022() {
        let (base, quote) = resolve_token_programs(3); // bit0=1, bit1=1
        assert_eq!(base, spl_token_2022::id());
        assert_eq!(quote, spl_token_2022::id());
    }

    #[test]
    fn user_atas_are_deterministic() {
        let keys = LaunchpadKeys {
            pool_id: Pubkey::new_unique(),
            authority: Pubkey::new_unique(),
            global_config: Pubkey::new_unique(),
            platform_config: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            base_token_program: spl_token::id(),
            quote_token_program: spl_token::id(),
        };
        let user = Pubkey::new_unique();

        let ata1 = keys.user_base_ata(&user);
        let ata2 = keys.user_base_ata(&user);
        assert_eq!(ata1, ata2);

        // Different mints → different ATAs
        let quote_ata = keys.user_quote_ata(&user);
        assert_ne!(ata1, quote_ata);
    }
}
