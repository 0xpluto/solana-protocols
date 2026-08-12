//! Derived position state for marking, fee accounting, in-range
//! checks. Joins a `PositionV2` (the static range + accumulators)
//! to the live [`LbPair`] + [`BinArray`]s pulled from the cache.
//!
//! All math here mirrors the on-chain program's `Position` impls
//! verbatim; it's a snapshot read, not a state transition.

use std::collections::HashMap;

use solana_program::pubkey::Pubkey;

use crate::protocols::meteora_dlmm::math::bin_array::{get_bin, is_bin_id_within_range};
use crate::protocols::meteora_dlmm::math::u128x128::{mul_div, Rounding};
use crate::protocols::meteora_dlmm::math::u64x64::SCALE_OFFSET;
use crate::protocols::meteora_dlmm::{Bin, BinArray, LbPair, PositionV2Full};

use super::coverage::bin_array_keys_for_position;

/// Whether the active bin sits inside the position's range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionRangeStatus {
    /// `lb_pair.active_id` is inside `[lower_bin_id, upper_bin_id]`
    /// — the position is earning fees on swaps.
    InRange,
    /// `active_id` is below `lower_bin_id`. Position is 100% on the
    /// X side (the lower-priced asset is the one held).
    BelowRange,
    /// `active_id` is above `upper_bin_id`. Position is 100% on the
    /// Y side.
    AboveRange,
}

/// Snapshot of derived position state. All amounts are in raw token
/// units (no decimal scaling).
#[derive(Debug, Clone)]
pub struct PositionMetrics {
    /// Sum of token X across every bin the position holds liquidity
    /// in, weighted by `liquidity_share / liquidity_supply`.
    pub current_value_x: u64,
    /// Same for token Y.
    pub current_value_y: u64,
    /// Uncollected fee on the X side accrued since last `claim_fee`.
    pub uncollected_fee_x: u64,
    /// Uncollected fee on the Y side.
    pub uncollected_fee_y: u64,
    /// In-range / above / below.
    pub range_status: PositionRangeStatus,
    /// Number of bins covered by the position
    /// (`upper_bin_id - lower_bin_id + 1`).
    pub range_width_bins: u32,
}

/// Compute [`PositionMetrics`] for a position by walking each bin in
/// its range. The caller is responsible for loading the bin arrays
/// covered by the position into `bin_arrays` (use
/// [`bin_array_keys_for_position`] to know which PDAs).
///
/// Errors if a bin array the position covers is missing from the
/// map, or if any inner math overflows. A *correct* call site would
/// load the cache snapshot first, then call this — there's no async
/// I/O in here.
pub fn compute_metrics(
    pool: &Pubkey,
    position: &PositionV2Full,
    pair: &LbPair,
    bin_arrays: &HashMap<Pubkey, BinArray>,
) -> Result<PositionMetrics, &'static str> {
    let mut current_value_x: u64 = 0;
    let mut current_value_y: u64 = 0;
    let mut fee_x: u64 = 0;
    let mut fee_y: u64 = 0;

    // Resolve which arrays we need to walk. `get_bin` errors if a
    // bin id falls outside the array we hand it, so we route each
    // bin to whichever array covers it.
    let array_keys = bin_array_keys_for_position(pool, &position.base)?;
    let arrays: Vec<&BinArray> = array_keys
        .iter()
        .map(|key| {
            bin_arrays
                .get(key)
                .ok_or("position bin array missing from cache snapshot")
        })
        .collect::<Result<_, _>>()?;

    for (offset, bin_id) in (position.base.lower_bin_id..=position.base.upper_bin_id).enumerate() {
        // Reads the inline array for the first 70 bins and the appended
        // records beyond, so wide (dynamic) positions are fully
        // accounted rather than truncated at 70.
        let share = position
            .liquidity_share(offset)
            .ok_or("position offset past decoded bins")?;
        if share == 0 {
            continue;
        }
        let array = pick_array_for_bin(&arrays, bin_id)?;
        let bin = get_bin(array, bin_id)?;

        // Token amounts owed to this share.
        let (x_amt, y_amt) = bin_share_to_amounts(bin, share)?;
        current_value_x = current_value_x
            .checked_add(x_amt)
            .ok_or("LBError::MathOverflow")?;
        current_value_y = current_value_y
            .checked_add(y_amt)
            .ok_or("LBError::MathOverflow")?;

        // Uncollected fees: bin's `fee_amount_x_per_token_stored` is
        // a Q64.64 cumulative-per-token-share. The position has two
        // checkpoints per side:
        //   - `fee_*_per_token_complete`: the last per-token rate
        //     folded into pending. Difference × share = newly
        //     accrued.
        //   - `fee_*_pending`: already-folded amount waiting to be
        //     claimed.
        // Total uncollected = newly_accrued + pending.
        let fee_info = position
            .fee_info(offset)
            .ok_or("position offset past decoded bins")?;
        fee_x = fee_x
            .checked_add(uncollected_fee(
                bin.fee_amount_x_per_token_stored,
                fee_info.fee_x_per_token_complete,
                share,
                fee_info.fee_x_pending,
            )?)
            .ok_or("LBError::MathOverflow")?;
        fee_y = fee_y
            .checked_add(uncollected_fee(
                bin.fee_amount_y_per_token_stored,
                fee_info.fee_y_per_token_complete,
                share,
                fee_info.fee_y_pending,
            )?)
            .ok_or("LBError::MathOverflow")?;
    }

    let range_status = if pair.active_id < position.base.lower_bin_id {
        PositionRangeStatus::BelowRange
    } else if pair.active_id > position.base.upper_bin_id {
        PositionRangeStatus::AboveRange
    } else {
        PositionRangeStatus::InRange
    };

    let range_width_bins = (position.base.upper_bin_id - position.base.lower_bin_id + 1) as u32;

    Ok(PositionMetrics {
        current_value_x,
        current_value_y,
        uncollected_fee_x: fee_x,
        uncollected_fee_y: fee_y,
        range_status,
        range_width_bins,
    })
}

/// Pick the array (out of `arrays`) that covers `bin_id`. Errors if
/// none does — caller's `arrays` slice should have come from
/// [`bin_array_keys_for_position`] which is exhaustive over the
/// position's range.
fn pick_array_for_bin<'a>(
    arrays: &[&'a BinArray],
    bin_id: i32,
) -> Result<&'a BinArray, &'static str> {
    arrays
        .iter()
        .copied()
        .find(|a| is_bin_id_within_range(a, bin_id))
        .ok_or("position bin id not covered by any cached bin array")
}

/// `(amount_x, amount_y)` owed to a `share` slice of the bin.
/// `liquidity_supply` is Q64.64; `share` is too. Token amounts are
/// integer.
fn bin_share_to_amounts(bin: &Bin, share: u128) -> Result<(u64, u64), &'static str> {
    if bin.liquidity_supply == 0 {
        return Ok((0, 0));
    }
    let x_amt = mul_div(
        share,
        u128::from(bin.amount_x),
        bin.liquidity_supply,
        Rounding::Down,
    )
    .ok_or("LBError::MathOverflow")?;
    let y_amt = mul_div(
        share,
        u128::from(bin.amount_y),
        bin.liquidity_supply,
        Rounding::Down,
    )
    .ok_or("LBError::MathOverflow")?;
    Ok((
        u64::try_from(x_amt).map_err(|_| "LBError::TypeCastFailed")?,
        u64::try_from(y_amt).map_err(|_| "LBError::TypeCastFailed")?,
    ))
}

/// Uncollected fee for a single bin slot.
///
/// `pending` already accounts for fees folded in at the last
/// position update; we add freshly-accrued (since the checkpoint
/// `position_per_token_complete`) by multiplying the per-token-stored
/// delta by the position's `share` and shifting out the Q64.64
/// fractional bits.
fn uncollected_fee(
    bin_per_token_stored: u128,
    position_per_token_complete: u128,
    share: u128,
    pending: u64,
) -> Result<u64, &'static str> {
    let newly_accrued = if bin_per_token_stored <= position_per_token_complete {
        0u64
    } else {
        let delta = bin_per_token_stored - position_per_token_complete;
        let raw = delta.checked_mul(share).ok_or("LBError::MathOverflow")?;
        let scaled = raw >> SCALE_OFFSET;
        u64::try_from(scaled).map_err(|_| "LBError::TypeCastFailed")?
    };
    pending
        .checked_add(newly_accrued)
        .ok_or("LBError::MathOverflow")
}
