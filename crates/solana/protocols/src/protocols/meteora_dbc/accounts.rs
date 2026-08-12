//! Meteora DBC account addresses and keys.
//!
//! DBC pools have a constant pool authority PDA shared across all pools.
//! The quote_mint is not stored in the VirtualPool state — it must be
//! provided externally (typically WSOL for meme coins).

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address_with_program_id;

use super::constants::{POOL_AUTHORITY, PROGRAM_ID};
use super::state::VirtualPool;

/// Keys for a Meteora DBC pool.
///
/// These are the addresses needed to build swap instructions.
#[derive(Debug, Clone)]
pub struct DbcKeys {
    /// VirtualPool account address.
    pub pool: Pubkey,

    /// Config account address.
    pub config: Pubkey,

    /// Base token mint.
    pub base_mint: Pubkey,

    /// Quote token mint (not stored in VirtualPool — must be provided).
    pub quote_mint: Pubkey,

    /// Base token vault.
    pub base_vault: Pubkey,

    /// Quote token vault.
    pub quote_vault: Pubkey,

    /// Base token program (SPL Token or Token2022).
    pub token_base_program: Pubkey,

    /// Quote token program.
    pub token_quote_program: Pubkey,
}

impl DbcKeys {
    /// Create keys from a pool state, its address, and external data.
    ///
    /// `quote_mint` is not stored in VirtualPool — caller must provide it.
    /// `pool_type` determines token programs: 0 = both SPL, 1 = base is Token2022.
    #[must_use]
    pub fn from_pool_state(pool_address: Pubkey, pool: &VirtualPool, quote_mint: Pubkey) -> Self {
        let (token_base_program, token_quote_program) = resolve_token_programs(pool.pool_type);

        Self {
            pool: pool_address,
            config: pool.config,
            base_mint: pool.base_mint,
            quote_mint,
            base_vault: pool.base_vault,
            quote_vault: pool.quote_vault,
            token_base_program,
            token_quote_program,
        }
    }

    /// Get the constant pool authority.
    #[must_use]
    pub fn pool_authority() -> Pubkey {
        POOL_AUTHORITY
    }

    /// Get the program ID.
    #[must_use]
    pub fn program_id() -> Pubkey {
        PROGRAM_ID
    }

    /// Derive event authority PDA (Anchor convention).
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
            &self.token_base_program,
        )
    }

    /// Derive the user's quote token ATA.
    #[must_use]
    pub fn user_quote_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.quote_mint,
            &self.token_quote_program,
        )
    }
}

/// Resolve token programs from pool_type flag.
///
/// pool_type 0 = both SPL Token.
/// pool_type 1 = base is Token2022, quote is SPL Token.
#[must_use]
pub fn resolve_token_programs(pool_type: u8) -> (Pubkey, Pubkey) {
    let base_program = if pool_type == 1 {
        spl_token_2022::id()
    } else {
        spl_token::id()
    };
    // Quote is always SPL Token (typically WSOL)
    let quote_program = spl_token::id();
    (base_program, quote_program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_keys() -> DbcKeys {
        DbcKeys {
            pool: Pubkey::new_unique(),
            config: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            quote_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            token_base_program: spl_token::id(),
            token_quote_program: spl_token::id(),
        }
    }

    #[test]
    fn program_id_matches() {
        assert_eq!(DbcKeys::program_id(), PROGRAM_ID);
    }

    #[test]
    fn pool_authority_matches() {
        assert_eq!(DbcKeys::pool_authority(), POOL_AUTHORITY);
    }

    #[test]
    fn event_authority_deterministic() {
        let ea1 = DbcKeys::event_authority();
        let ea2 = DbcKeys::event_authority();
        assert_eq!(ea1, ea2);
        assert_ne!(ea1, Pubkey::default());
    }

    #[test]
    fn user_atas_deterministic() {
        let keys = make_test_keys();
        let user = Pubkey::new_unique();

        let ata_base = keys.user_base_ata(&user);
        let ata_quote = keys.user_quote_ata(&user);
        assert_ne!(ata_base, ata_quote);

        // Same user → same ATA
        assert_eq!(ata_base, keys.user_base_ata(&user));
    }

    #[test]
    fn resolve_spl_token() {
        let (base, quote) = resolve_token_programs(0);
        assert_eq!(base, spl_token::id());
        assert_eq!(quote, spl_token::id());
    }

    #[test]
    fn resolve_token2022() {
        let (base, quote) = resolve_token_programs(1);
        assert_eq!(base, spl_token_2022::id());
        assert_eq!(quote, spl_token::id());
    }

    #[test]
    fn from_pool_state() {
        let pool = super::super::state::VirtualPool {
            volatility_tracker: super::super::state::VolatilityTracker::default(),
            config: Pubkey::new_unique(),
            creator: Pubkey::new_unique(),
            base_mint: Pubkey::new_unique(),
            base_vault: Pubkey::new_unique(),
            quote_vault: Pubkey::new_unique(),
            base_reserve: 1_000_000_000,
            quote_reserve: 500_000_000,
            protocol_base_fee: 0,
            protocol_quote_fee: 0,
            partner_base_fee: 0,
            partner_quote_fee: 0,
            sqrt_price: 1u128 << 64,
            activation_point: 0,
            pool_type: 0,
            is_migrated: 0,
            is_partner_withdraw_surplus: 0,
            is_protocol_withdraw_surplus: 0,
            migration_progress: 0,
            is_withdraw_leftover: 0,
            is_creator_withdraw_surplus: 0,
            migration_fee_withdraw_status: 0,
            metrics: super::super::state::PoolMetrics::default(),
            finish_curve_timestamp: 0,
            creator_base_fee: 0,
            creator_quote_fee: 0,
            legacy_creation_fee_bits: 0,
            creation_fee_bits: 0,
            _padding_0: [0u8; 6],
            _padding_1: [0u64; 6],
        };
        let pool_address = Pubkey::new_unique();
        let quote_mint = Pubkey::new_unique();

        let keys = DbcKeys::from_pool_state(pool_address, &pool, quote_mint);
        assert_eq!(keys.pool, pool_address);
        assert_eq!(keys.config, pool.config);
        assert_eq!(keys.base_mint, pool.base_mint);
        assert_eq!(keys.quote_mint, quote_mint);
        assert_eq!(keys.base_vault, pool.base_vault);
        assert_eq!(keys.quote_vault, pool.quote_vault);
        assert_eq!(keys.token_base_program, spl_token::id());
    }
}
