//! Free-function ports of the methods the on-chain DLMM program
//! defines on its `LbPair` and `VariableParameters` structs.
//!
//! Why free functions instead of an extension trait: the bin-walk
//! quoter takes an owned mutable clone of [`LbPair`] and updates
//! its volatility parameters as it crosses bins. Free functions
//! taking `&mut LbPair` keep the call sites uncluttered and let
//! us call SDK-internal field accessors without a trait import.
//!
//! Source of truth: the on-chain program's own `LbPair` implementation.

use super::super::constants::{BASIS_POINT_MAX, FEE_PRECISION, MAX_FEE_RATE};
use super::super::state::LbPair;
use super::safe_math::SafeMath;

/// Base fee in `FEE_PRECISION` units. Includes the `base_fee_power_factor`
/// multiplier (which scales `base_fee` by `10^power_factor`).
pub fn get_base_fee(pair: &LbPair) -> Result<u128, &'static str> {
    let base = u128::from(pair.parameters.base_factor)
        .safe_mul(pair.bin_step.into())?
        .safe_mul(10u128)?;
    let power = pair.parameters.base_fee_power_factor;
    if power == 0 {
        Ok(base)
    } else {
        let scale = 10u128
            .checked_pow(power as u32)
            .ok_or("LBError::MathOverflow")?;
        base.safe_mul(scale)
    }
}

/// Variable fee given a volatility accumulator value, in
/// `FEE_PRECISION` units.
pub fn compute_variable_fee(
    pair: &LbPair,
    volatility_accumulator: u32,
) -> Result<u128, &'static str> {
    if pair.parameters.variable_fee_control == 0 {
        return Ok(0);
    }
    let va = u128::from(volatility_accumulator);
    let bs = u128::from(pair.bin_step);
    let vfc = u128::from(pair.parameters.variable_fee_control);

    let square_vfa_bin = va
        .safe_mul(bs)?
        .checked_pow(2)
        .ok_or("LBError::MathOverflow")?;
    let v_fee = vfc.safe_mul(square_vfa_bin)?;

    // 1e20 → 1e9 with ceiling.
    v_fee.safe_add(99_999_999_999)?.safe_div(100_000_000_000)
}

/// Variable fee for the *current* volatility accumulator on the pair.
pub fn get_variable_fee(pair: &LbPair) -> Result<u128, &'static str> {
    compute_variable_fee(pair, pair.v_parameters.volatility_accumulator)
}

/// Total fee rate in `FEE_PRECISION` units (base + variable, capped
/// at [`MAX_FEE_RATE`]).
pub fn get_total_fee(pair: &LbPair) -> Result<u128, &'static str> {
    let total = get_base_fee(pair)?.safe_add(get_variable_fee(pair)?)?;
    Ok(total.min(MAX_FEE_RATE as u128))
}

/// Fee on a swap-input amount that doesn't include fees yet (caller's
/// `amount_in` is what the user supplied; we charge `fee` on top).
/// Ceil-divides to match on-chain rounding.
pub fn compute_fee(pair: &LbPair, amount: u64) -> Result<u64, &'static str> {
    let total_fee_rate = get_total_fee(pair)?;
    let denominator = u128::from(FEE_PRECISION).safe_sub(total_fee_rate)?;
    let fee = u128::from(amount)
        .safe_mul(total_fee_rate)?
        .safe_add(denominator)?
        .safe_sub(1)?;
    let scaled_down_fee = fee.safe_div(denominator)?;
    scaled_down_fee
        .try_into()
        .map_err(|_| "LBError::TypeCastFailed")
}

/// Fee included in an `amount_with_fees` value (caller's amount
/// already has the fee baked in; we want to extract it).
pub fn compute_fee_from_amount(pair: &LbPair, amount_with_fees: u64) -> Result<u64, &'static str> {
    let total_fee_rate = get_total_fee(pair)?;
    let fee_amount = u128::from(amount_with_fees)
        .safe_mul(total_fee_rate)?
        .safe_add((FEE_PRECISION - 1).into())?;
    let scaled_down_fee = fee_amount.safe_div(FEE_PRECISION.into())?;
    scaled_down_fee
        .try_into()
        .map_err(|_| "LBError::TypeCastFailed")
}

/// Protocol's share of a fee amount.
pub fn compute_protocol_fee(pair: &LbPair, fee_amount: u64) -> Result<u64, &'static str> {
    let protocol_fee = u128::from(fee_amount)
        .safe_mul(pair.parameters.protocol_share.into())?
        .safe_div(BASIS_POINT_MAX as u128)?;
    protocol_fee
        .try_into()
        .map_err(|_| "LBError::TypeCastFailed")
}

// ---------------------------------------------------------------------------
// Variable parameter updates — mutate the pair as part of a quote.
// ---------------------------------------------------------------------------

/// Update `index_reference` and `volatility_reference` based on the
/// time elapsed since the previous trade. Three regimes:
///
/// * `elapsed < filter_period`: high-frequency trade, no update.
/// * `filter_period <= elapsed < decay_period`: decay
///   `volatility_reference` by `reduction_factor / BASIS_POINT_MAX`.
/// * `elapsed >= decay_period`: zero out `volatility_reference`.
pub fn update_references(pair: &mut LbPair, current_timestamp: i64) -> Result<(), &'static str> {
    let elapsed = current_timestamp.safe_sub(pair.v_parameters.last_update_timestamp)?;
    if elapsed >= pair.parameters.filter_period as i64 {
        pair.v_parameters.index_reference = pair.active_id;
        if elapsed < pair.parameters.decay_period as i64 {
            let volatility_reference = pair
                .v_parameters
                .volatility_accumulator
                .safe_mul(pair.parameters.reduction_factor as u32)?
                .safe_div(BASIS_POINT_MAX as u32)?;
            pair.v_parameters.volatility_reference = volatility_reference;
        } else {
            pair.v_parameters.volatility_reference = 0;
        }
    }
    Ok(())
}

/// Re-compute `volatility_accumulator` from
/// `volatility_reference + |index_reference - active_id| * BASIS_POINT_MAX`,
/// capped at `max_volatility_accumulator`.
pub fn update_volatility_accumulator(pair: &mut LbPair) -> Result<(), &'static str> {
    let delta_id = i64::from(pair.v_parameters.index_reference)
        .safe_sub(pair.active_id.into())?
        .unsigned_abs();
    let volatility_accumulator = u64::from(pair.v_parameters.volatility_reference)
        .safe_add(delta_id.safe_mul(BASIS_POINT_MAX as u64)?)?;
    pair.v_parameters.volatility_accumulator = volatility_accumulator
        .min(pair.parameters.max_volatility_accumulator.into())
        .try_into()
        .map_err(|_| "LBError::TypeCastFailed")?;
    Ok(())
}

/// Move the active bin one step in the swap direction.
pub fn advance_active_bin(pair: &mut LbPair, swap_for_y: bool) -> Result<(), &'static str> {
    use super::super::constants::{MAX_BIN_ID, MIN_BIN_ID};
    let next = if swap_for_y {
        pair.active_id.safe_sub(1)?
    } else {
        pair.active_id.safe_add(1)?
    };
    if !(MIN_BIN_ID..=MAX_BIN_ID).contains(&next) {
        return Err("LBError::BinIdOutOfRange");
    }
    pair.active_id = next;
    Ok(())
}
