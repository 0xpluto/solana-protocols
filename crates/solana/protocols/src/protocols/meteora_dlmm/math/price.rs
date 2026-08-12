//! Bin-id → Q64.64 price.

use super::super::constants::BASIS_POINT_MAX;
use super::safe_math::SafeMath;
use super::u64x64::{pow, ONE, SCALE_OFFSET};

/// Compute the Q64.64 price of a given bin from `(active_id,
/// bin_step)`. `bin_step` is in basis points
/// (`bin_step = 1` means each bin is 0.01% wider than the previous).
///
/// Formula: `price = (1 + bin_step/10_000)^active_id`, evaluated in
/// Q64.64.
pub fn get_price_from_id(active_id: i32, bin_step: u16) -> Result<u128, &'static str> {
    let bps = u128::from(bin_step)
        .safe_shl(SCALE_OFFSET.into())?
        .safe_div(BASIS_POINT_MAX as u128)?;
    let base = ONE.safe_add(bps)?;
    pow(base, active_id).ok_or("LBError::MathOverflow")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_id_zero_is_one() {
        // (1.0001)^0 = 1.0 in Q64.64.
        assert_eq!(get_price_from_id(0, 1).unwrap(), ONE);
    }

    #[test]
    fn positive_active_id_above_one() {
        // For positive active_id the price > 1.0.
        let price = get_price_from_id(100, 10).unwrap();
        assert!(price > ONE);
    }

    #[test]
    fn negative_active_id_below_one() {
        let price = get_price_from_id(-100, 10).unwrap();
        assert!(price < ONE);
        assert!(price > 0);
    }
}
