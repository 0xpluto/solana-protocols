//! Resolve which `BinArray` PDAs a position covers.
//!
//! A `PositionV2` lives in `[lower_bin_id, upper_bin_id]` (inclusive).
//! `MAX_BIN_PER_ARRAY = 70`, so a single position spans 1 or 2
//! arrays — the on-chain `add_liquidity*` / `claim_*` ixs need both
//! as `remaining_accounts`. For ranges that exactly straddle an
//! array boundary you'll get two PDAs back.

use solana_program::pubkey::Pubkey;

use crate::protocols::meteora_dlmm::accounts::derive_bin_array_address;
use crate::protocols::meteora_dlmm::math::bin_array::bin_id_to_bin_array_index;
use crate::protocols::meteora_dlmm::PositionV2;

/// PDAs for the bin arrays a position currently covers.
pub fn bin_array_keys_for_position(
    pool: &Pubkey,
    position: &PositionV2,
) -> Result<Vec<Pubkey>, &'static str> {
    bin_array_keys_for_range(pool, position.lower_bin_id, position.upper_bin_id)
}

/// PDAs for the bin arrays an `[lower, upper]` (inclusive) range
/// covers. Returns 1 or 2 PDAs depending on whether the range
/// straddles a bin-array boundary.
pub fn bin_array_keys_for_range(
    pool: &Pubkey,
    lower_bin_id: i32,
    upper_bin_id: i32,
) -> Result<Vec<Pubkey>, &'static str> {
    if upper_bin_id < lower_bin_id {
        return Err("LBError::InvalidBinId");
    }
    let lower_idx = bin_id_to_bin_array_index(lower_bin_id)?;
    let upper_idx = bin_id_to_bin_array_index(upper_bin_id)?;
    let mut out = Vec::with_capacity(2);
    for idx in lower_idx..=upper_idx {
        out.push(derive_bin_array_address(pool, idx as i64));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    #[test]
    fn single_array_range_resolves_to_one_pda() {
        let pool = pk(0xAA);
        let keys = bin_array_keys_for_range(&pool, 0, 50).unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn straddling_range_resolves_to_two_pdas() {
        let pool = pk(0xAA);
        // Bins 60..=80 straddle array 0 (0..=69) and array 1 (70..=139).
        let keys = bin_array_keys_for_range(&pool, 60, 80).unwrap();
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1]);
    }

    #[test]
    fn negative_to_positive_range_spans_arrays() {
        let pool = pk(0xAA);
        let keys = bin_array_keys_for_range(&pool, -10, 10).unwrap();
        // Array -1 (covers -70..=-1) and array 0 (covers 0..=69).
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn inverted_range_errors() {
        let pool = pk(0xAA);
        assert!(bin_array_keys_for_range(&pool, 10, 5).is_err());
    }
}
