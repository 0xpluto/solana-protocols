//! `Q64.64` fixed-point exponentiation used to compute bin prices
//! from `(active_id, bin_step)`. Ported from the on-chain Meteora
//! DLMM (`lb_clmm`) program's `u64x64_math`.
//!
//! The exponentiation unrolls all 19 iterations of the binary
//! exponential by hand because that's enough to cover the full
//! `[MIN_BIN_ID, MAX_BIN_ID]` range — see the comment on
//! `MAX_EXPONENTIAL` for the derivation.

use ruint::aliases::U256;

use super::super::constants::BASIS_POINT_MAX;

/// Decimal precision when converting fixed-point ↔ decimal (`10^12`).
pub const PRECISION: u128 = 1_000_000_000_000;

/// Bits to scale by — equivalent to the radix-point position.
pub const SCALE_OFFSET: u8 = 64;

/// `1.0` in Q64.64.
pub const ONE: u128 = 1u128 << SCALE_OFFSET;

/// Above this exponent the result overflows the Q64.64 range. The
/// bound is derived from the smallest bin step (1bp): `(1 + 0.0001)^n
/// < 2^64` solves for `n ≈ 443_636`, which fits in 19 bits;
/// `0x80000` is the next bit and signals "no point continuing".
const MAX_EXPONENTIAL: u32 = 0x80000;

/// `base^exp` in Q64.64. Returns `None` on overflow / underflow.
///
/// Negative exponents are handled by inverting the base first
/// (`u128::MAX / base`), which keeps the upper 64 bits zero so
/// the squared multiplications don't overflow `u128`.
pub fn pow(base: u128, exp: i32) -> Option<u128> {
    let mut invert = exp.is_negative();

    if exp == 0 {
        return Some(1u128 << 64);
    }

    let exp: u32 = if invert {
        exp.unsigned_abs()
    } else {
        exp as u32
    };

    if exp >= MAX_EXPONENTIAL {
        return None;
    }

    let mut squared_base = base;
    let mut result = ONE;

    if squared_base >= result {
        squared_base = u128::MAX.checked_div(squared_base)?;
        invert = !invert;
    }

    if exp & 0x1 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x2 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x4 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x8 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x10 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x20 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x40 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x80 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x100 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x200 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x400 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x800 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x1000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x2000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x4000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x8000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x10000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x20000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }
    squared_base = (squared_base.checked_mul(squared_base)?) >> SCALE_OFFSET;
    if exp & 0x40000 > 0 {
        result = (result.checked_mul(squared_base)?) >> SCALE_OFFSET;
    }

    if result == 0 {
        return None;
    }

    if invert {
        result = u128::MAX.checked_div(result)?;
    }

    Some(result)
}

/// Convert Q64.64 fixed-point to a decimal scaled by `PRECISION`
/// (`10^12`). UI-only; never used in on-chain math.
pub fn to_decimal(value: u128) -> Option<u128> {
    let value = U256::from(value);
    let precision = U256::from(PRECISION);
    let scaled_value = value.checked_mul(precision)?;
    let (scaled_down_value, _) = scaled_value.overflowing_shr(SCALE_OFFSET.into());
    scaled_down_value.try_into().ok()
}

/// Inverse of [`to_decimal`].
pub fn from_decimal(value: u128) -> Option<u128> {
    let value = U256::from(value);
    let precision = U256::from(PRECISION);
    let (q_value, _) = value.overflowing_shl(SCALE_OFFSET.into());
    let fp_value = q_value.checked_div(precision)?;
    fp_value.try_into().ok()
}

/// Q64.64 representation of `1 + bin_step / 10_000` — the per-bin
/// price multiplier.
pub fn get_base(bin_step: u32) -> Option<u128> {
    let quotient = u128::from(bin_step).checked_shl(SCALE_OFFSET.into())?;
    let fraction = quotient.checked_div(BASIS_POINT_MAX as u128)?;
    ONE.checked_add(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_is_q64_one() {
        // 1.0 in Q64.64 == 2^64.
        assert_eq!(ONE, 1u128 << 64);
    }

    #[test]
    fn pow_zero_is_one() {
        assert_eq!(pow(get_base(10).unwrap(), 0), Some(ONE));
    }

    #[test]
    fn decimal_roundtrip() {
        // ~1.5 in Q64.64 should round-trip with PRECISION.
        let original = ONE + (ONE >> 1); // 1.5
        let decimal = to_decimal(original).unwrap();
        let back = from_decimal(decimal).unwrap();
        // Allow 1-bit rounding error from the to_decimal shift.
        let diff = back.max(original) - back.min(original);
        assert!(diff <= 1);
    }
}
