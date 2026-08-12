//! PDA derivation + reusable key bundles for Meteora DLMM.
//!
//! The codama-generated SDK ships every account *layout* but no PDA
//! derivers. Everything in this module is hand-written.

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address_with_program_id;

use super::constants::{EVENT_AUTHORITY, PROGRAM_ID};
use super::state::LbPair;
use crate::dlmm_sdk_pubkey;

/// Bin array index → byte representation used by the on-chain program.
///
/// Bin array indices are **i64** big-endian on-chain, but the program
/// signs PDAs with little-endian bytes. (DLMM convention.)
fn bin_array_index_seed(index: i64) -> [u8; 8] {
    index.to_le_bytes()
}

/// Derive a `BinArray` PDA from its [`LbPair`] and bin-array index.
///
/// Seeds: `[b"bin_array", lb_pair, index_le_bytes]`. Index is signed
/// because the bin-id space is `[-443_636, 443_636]` and arrays are
/// numbered from the bin-id divided by [`MAX_BIN_PER_ARRAY`].
///
/// [`MAX_BIN_PER_ARRAY`]: super::constants::MAX_BIN_PER_ARRAY
#[must_use]
pub fn derive_bin_array_address(lb_pair: &Pubkey, index: i64) -> Pubkey {
    Pubkey::find_program_address(
        &[b"bin_array", lb_pair.as_ref(), &bin_array_index_seed(index)],
        &PROGRAM_ID,
    )
    .0
}

/// Derive the `BinArrayBitmapExtension` PDA for an `LbPair`.
///
/// Seeds: `[b"bitmap", lb_pair]`. Optional account — only some pools
/// have one (the inline 16×u64 bitmap covers ±6720 bins; outside that
/// range the program reaches into this extension).
#[must_use]
pub fn derive_bin_array_bitmap_extension(lb_pair: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bitmap", lb_pair.as_ref()], &PROGRAM_ID).0
}

/// Derive the `Oracle` PDA for an `LbPair`.
///
/// Seeds: `[b"oracle", lb_pair]`.
#[must_use]
pub fn derive_oracle_address(lb_pair: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"oracle", lb_pair.as_ref()], &PROGRAM_ID).0
}

/// Derive the `PositionV2` PDA used by `initialize_position_pda`.
///
/// Seeds: `[b"position", lb_pair, lower_bin_id_le, width_le]`. Used
/// when a deterministic position address is needed (e.g. so the same
/// owner can re-derive their position without persisting the address).
#[must_use]
pub fn derive_position_pda(lb_pair: &Pubkey, lower_bin_id: i32, width: i32) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"position",
            lb_pair.as_ref(),
            &lower_bin_id.to_le_bytes(),
            &width.to_le_bytes(),
        ],
        &PROGRAM_ID,
    )
    .0
}

/// Derive the Anchor `event_authority` PDA. Constant for the program;
/// equals [`EVENT_AUTHORITY`]. The function exists so callers don't
/// have to import the constant when they're already pulling
/// `accounts::*`.
#[must_use]
pub fn event_authority() -> Pubkey {
    EVENT_AUTHORITY
}

/// Bundle of pool addresses needed to build any swap instruction.
///
/// `bin_array` accounts are *not* part of this struct — they're
/// passed as `remaining_accounts` and depend on the swap path
/// (active bin and direction). See [`derive_bin_array_address`].
#[derive(Debug, Clone)]
pub struct DlmmKeys {
    /// LB pair (pool) account.
    pub lb_pair: Pubkey,
    /// Optional bitmap extension; only set for sparse pools.
    pub bin_array_bitmap_extension: Pubkey,
    /// Token X mint.
    pub token_x_mint: Pubkey,
    /// Token Y mint.
    pub token_y_mint: Pubkey,
    /// Token X reserve (vault).
    pub reserve_x: Pubkey,
    /// Token Y reserve (vault).
    pub reserve_y: Pubkey,
    /// Token X program id (SPL or Token-2022).
    pub token_x_program: Pubkey,
    /// Token Y program id.
    pub token_y_program: Pubkey,
    /// Oracle account.
    pub oracle: Pubkey,
}

impl DlmmKeys {
    /// Build a key bundle from a freshly decoded [`LbPair`] and its
    /// on-chain address. The pool *contains* the reserve / mint /
    /// oracle pubkeys; the caller still has to supply token-program
    /// ids (SPL vs Token-2022) and the bitmap extension address (if
    /// the pool has one) since neither is stored on the pair.
    #[must_use]
    pub fn from_pool_state(
        lb_pair: Pubkey,
        pool: &LbPair,
        bin_array_bitmap_extension: Pubkey,
        token_x_program: Pubkey,
        token_y_program: Pubkey,
    ) -> Self {
        Self {
            lb_pair,
            bin_array_bitmap_extension,
            token_x_mint: dlmm_sdk_pubkey!(pool.token_x_mint),
            token_y_mint: dlmm_sdk_pubkey!(pool.token_y_mint),
            reserve_x: dlmm_sdk_pubkey!(pool.reserve_x),
            reserve_y: dlmm_sdk_pubkey!(pool.reserve_y),
            token_x_program,
            token_y_program,
            oracle: dlmm_sdk_pubkey!(pool.oracle),
        }
    }

    /// Program id helper.
    #[must_use]
    pub fn program_id() -> Pubkey {
        PROGRAM_ID
    }

    /// Event-authority PDA helper.
    #[must_use]
    pub fn event_authority() -> Pubkey {
        EVENT_AUTHORITY
    }

    /// User's token-X ATA, scoped to the pool's `token_x_program`.
    #[must_use]
    pub fn user_token_x_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.token_x_mint,
            &self.token_x_program,
        )
    }

    /// User's token-Y ATA, scoped to the pool's `token_y_program`.
    #[must_use]
    pub fn user_token_y_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(
            user,
            &self.token_y_mint,
            &self.token_y_program,
        )
    }

    /// Convenience wrapper around [`derive_bin_array_address`].
    #[must_use]
    pub fn derive_bin_array(&self, index: i64) -> Pubkey {
        derive_bin_array_address(&self.lb_pair, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_array_pda_deterministic_per_index() {
        let pair = Pubkey::new_unique();
        let a = derive_bin_array_address(&pair, 0);
        let b = derive_bin_array_address(&pair, 0);
        assert_eq!(a, b);
        assert_ne!(a, derive_bin_array_address(&pair, 1));
        // Negative indices are valid.
        assert_ne!(a, derive_bin_array_address(&pair, -1));
    }

    #[test]
    fn bitmap_extension_pda_per_pair() {
        let pair = Pubkey::new_unique();
        assert_ne!(
            derive_bin_array_bitmap_extension(&pair),
            derive_oracle_address(&pair)
        );
    }

    #[test]
    fn position_pda_varies_with_range() {
        let pair = Pubkey::new_unique();
        assert_ne!(
            derive_position_pda(&pair, -1000, 70),
            derive_position_pda(&pair, -1000, 35)
        );
        assert_ne!(
            derive_position_pda(&pair, -1000, 70),
            derive_position_pda(&pair, -930, 70)
        );
    }

    #[test]
    fn event_authority_constant_matches() {
        assert_eq!(event_authority(), EVENT_AUTHORITY);
    }
}
