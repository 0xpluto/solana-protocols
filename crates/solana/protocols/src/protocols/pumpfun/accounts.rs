//! Pump.fun account derivations and key management.
//!
//! This module handles PDA derivation for all pump.fun accounts
//! and provides the [`PumpfunKeys`] struct for instruction building.

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;

use super::constants::{
    BONDING_CURVE_SEED, BONDING_CURVE_V2_SEED, CREATOR_VAULT_SEED, PROGRAM_ID,
    USER_VOLUME_ACCUMULATOR_SEED,
};

/// All keys needed to build pump.fun swap instructions.
///
/// This struct contains all the derived addresses needed for
/// buy/sell operations. Create it once and reuse for multiple swaps.
#[derive(Debug, Clone)]
pub struct PumpfunKeys {
    /// Token mint address.
    pub mint: Pubkey,
    /// Token creator address.
    pub creator: Pubkey,
    /// Bonding curve PDA.
    pub bonding_curve: Pubkey,
    /// Associated token account for bonding curve.
    pub associated_bonding_curve: Pubkey,
    /// Creator vault PDA.
    pub creator_vault: Pubkey,
}

impl PumpfunKeys {
    /// Create keys from mint and creator addresses.
    ///
    /// Derives all necessary PDAs automatically.
    #[must_use]
    pub fn new(mint: Pubkey, creator: Pubkey) -> Self {
        let bonding_curve = derive_bonding_curve_pda(&mint);
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, &mint);
        let creator_vault = derive_creator_vault_pda(&creator);

        PumpfunKeys {
            mint,
            creator,
            bonding_curve,
            associated_bonding_curve,
            creator_vault,
        }
    }

    /// Get the user's associated token account for this mint.
    #[must_use]
    pub fn user_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address(user, &self.mint)
    }

    /// Get the user's volume accumulator PDA.
    #[must_use]
    pub fn user_volume_accumulator(&self, user: &Pubkey) -> Pubkey {
        derive_user_volume_accumulator_pda(user)
    }
}

/// Derive the bonding curve PDA for a token mint.
///
/// The bonding curve account stores the pool state including
/// virtual reserves and completion status.
#[must_use]
pub fn derive_bonding_curve_pda(mint: &Pubkey) -> Pubkey {
    let (pda, _bump) =
        Pubkey::find_program_address(&[BONDING_CURVE_SEED, mint.as_ref()], &PROGRAM_ID);
    pda
}

/// Derive the `bonding_curve_v2` PDA appended to buy/sell account lists.
///
/// Every one of 33 tailed mainnet instructions carried it, at a position that
/// varies — so a consumer finds it by deriving it, not by reading a slot.
#[must_use]
pub fn derive_bonding_curve_v2_pda(mint: &Pubkey) -> Pubkey {
    let (pda, _bump) =
        Pubkey::find_program_address(&[BONDING_CURVE_V2_SEED, mint.as_ref()], &PROGRAM_ID);
    pda
}

/// Derive the creator vault PDA for a token creator.
///
/// The creator vault receives creator fees from swaps.
#[must_use]
pub fn derive_creator_vault_pda(creator: &Pubkey) -> Pubkey {
    let (pda, _bump) =
        Pubkey::find_program_address(&[CREATOR_VAULT_SEED, creator.as_ref()], &PROGRAM_ID);
    pda
}

/// Derive the user volume accumulator PDA.
///
/// Tracks cumulative volume for fee discounts. A PDA of the **pumpfun program
/// itself**, seeds `["user_volume_accumulator", user]` — pinned against a real
/// landed buy (previously mis-derived under the deleted `FEE_PROGRAM_ID` with a
/// `"user-volume"` seed, yielding a key matching nothing on-chain).
#[must_use]
pub fn derive_user_volume_accumulator_pda(user: &Pubkey) -> Pubkey {
    let (pda, _bump) =
        Pubkey::find_program_address(&[USER_VOLUME_ACCUMULATOR_SEED, user.as_ref()], &PROGRAM_ID);
    pda
}

/// Get the associated token account for the bonding curve.
///
/// This is where the bonding curve holds its token reserves.
#[must_use]
pub fn derive_associated_bonding_curve(mint: &Pubkey) -> Pubkey {
    let bonding_curve = derive_bonding_curve_pda(mint);
    get_associated_token_address(&bonding_curve, mint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pda_derivation_deterministic() {
        let mint = Pubkey::new_unique();

        // Same input should produce same output
        let pda1 = derive_bonding_curve_pda(&mint);
        let pda2 = derive_bonding_curve_pda(&mint);
        assert_eq!(pda1, pda2);
    }

    #[test]
    fn pumpfun_keys_derives_all() {
        let mint = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let user = Pubkey::new_unique();

        let keys = PumpfunKeys::new(mint, creator);

        // Verify all keys are derived correctly
        assert_eq!(keys.mint, mint);
        assert_eq!(keys.creator, creator);
        assert_eq!(keys.bonding_curve, derive_bonding_curve_pda(&mint));
        assert_eq!(
            keys.associated_bonding_curve,
            get_associated_token_address(&keys.bonding_curve, &mint)
        );
        assert_eq!(keys.creator_vault, derive_creator_vault_pda(&creator));

        // User-specific derivations
        let user_ata = keys.user_ata(&user);
        assert_eq!(user_ata, get_associated_token_address(&user, &mint));
    }
}
