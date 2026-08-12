//! Per-bin swap math. The SDK ships [`Bin`] as a borsh struct with
//! the same field layout as the on-chain `Bin`; these free functions
//! re-implement the methods the on-chain program defines on it.
//!
//! Source: the on-chain program's own `impl Bin` block and `SwapResult`.

use super::super::constants::BASIS_POINT_MAX;
use super::super::state::{Bin, LbPair};
use super::lb_pair as lb_math;
use super::safe_math::SafeMath;
use super::u128x128::Rounding;
use super::u64x64::SCALE_OFFSET;
use super::util::{safe_mul_shr_cast, safe_shl_div_cast};

/// Outcome of swapping into one bin.
#[derive(Debug, Clone, Copy)]
pub struct BinSwapResult {
    /// Amount swapped into the bin including fees.
    pub amount_in_with_fees: u64,
    /// Amount swapped out of the bin (the user receives this).
    pub amount_out: u64,
    /// Total fee charged (includes protocol portion).
    pub fee: u64,
    /// Fee retained by the protocol after host fee carve-out.
    pub protocol_fee_after_host_fee: u64,
    /// Host fee portion (0 unless `host_fee_bps` was supplied).
    pub host_fee: u64,
    /// `true` if this swap satisfied an exact-out request inside the
    /// current bin (no further crossing needed).
    pub is_exact_out_amount: bool,
}

/// Maximum amount the user can receive from this bin, in its
/// outgoing token.
#[inline]
pub fn max_amount_out(bin: &Bin, swap_for_y: bool) -> u64 {
    if swap_for_y {
        bin.amount_y
    } else {
        bin.amount_x
    }
}

/// Maximum amount the user must put in to drain the entire opposite
/// reserve at the current `price`. Ceil-rounded.
pub fn max_amount_in(bin: &Bin, price: u128, swap_for_y: bool) -> Result<u64, &'static str> {
    if swap_for_y {
        // amount_y / price (Q64.64 → integer)
        safe_shl_div_cast(bin.amount_y.into(), price, SCALE_OFFSET, Rounding::Up)
    } else {
        // amount_x * price (integer * Q64.64 → integer with shr)
        safe_mul_shr_cast(bin.amount_x.into(), price, SCALE_OFFSET, Rounding::Up)
    }
}

/// Output amount for a given input at this bin's price (no fee).
pub fn get_amount_out(amount_in: u64, price: u128, swap_for_y: bool) -> Result<u64, &'static str> {
    if swap_for_y {
        // X → Y: amount_in * price
        safe_mul_shr_cast(price, amount_in.into(), SCALE_OFFSET, Rounding::Down)
    } else {
        // Y → X: amount_in / price
        safe_shl_div_cast(amount_in.into(), price, SCALE_OFFSET, Rounding::Down)
    }
}

/// Required input amount for a given output at this bin's price.
/// Ceil-rounded so the protocol never under-charges.
pub fn get_amount_in(amount_out: u64, price: u128, swap_for_y: bool) -> Result<u64, &'static str> {
    if swap_for_y {
        // X → Y: amount_out / price
        safe_shl_div_cast(amount_out.into(), price, SCALE_OFFSET, Rounding::Up)
    } else {
        // Y → X: amount_out * price
        safe_mul_shr_cast(amount_out.into(), price, SCALE_OFFSET, Rounding::Up)
    }
}

/// True iff the bin has zero of whichever side the swap is buying.
#[inline]
pub fn is_empty(bin: &Bin, side_in: bool) -> bool {
    if side_in {
        bin.amount_x == 0
    } else {
        bin.amount_y == 0
    }
}

/// Lazily compute (and cache) the bin price. Modifies `bin.price` if
/// it was zero.
pub fn get_or_store_bin_price(bin: &mut Bin, id: i32, bin_step: u16) -> Result<u128, &'static str> {
    if bin.price == 0 {
        bin.price = super::price::get_price_from_id(id, bin_step)?;
    }
    Ok(bin.price)
}

/// Swap into this bin until either (a) `amount_in` is consumed, or
/// (b) the opposite-side reserve is fully drained. Mutates the bin
/// to reflect the swap (`amount_x` / `amount_y` updated).
///
/// `host_fee_bps` carves the host portion out of the protocol fee.
pub fn bin_swap(
    bin: &mut Bin,
    amount_in: u64,
    price: u128,
    swap_for_y: bool,
    pair: &LbPair,
    host_fee_bps: Option<u16>,
) -> Result<BinSwapResult, &'static str> {
    let bin_max_amount_out = max_amount_out(bin, swap_for_y);
    let mut bin_max_amount_in = max_amount_in(bin, price, swap_for_y)?;
    let max_fee = lb_math::compute_fee(pair, bin_max_amount_in)?;
    bin_max_amount_in = bin_max_amount_in.safe_add(max_fee)?;

    let (amount_in_with_fees, amount_out, fee, protocol_fee) = if amount_in > bin_max_amount_in {
        // Drains the bin.
        (
            bin_max_amount_in,
            bin_max_amount_out,
            max_fee,
            lb_math::compute_protocol_fee(pair, max_fee)?,
        )
    } else {
        let fee = lb_math::compute_fee_from_amount(pair, amount_in)?;
        let amount_in_after_fee = amount_in.safe_sub(fee)?;
        let amount_out = get_amount_out(amount_in_after_fee, price, swap_for_y)?;
        (
            amount_in,
            std::cmp::min(amount_out, bin_max_amount_out),
            fee,
            lb_math::compute_protocol_fee(pair, fee)?,
        )
    };

    let host_fee = match host_fee_bps {
        Some(bps) => protocol_fee
            .safe_mul(bps.into())?
            .safe_div(BASIS_POINT_MAX as u64)?,
        None => 0,
    };
    let protocol_fee_after_host_fee = protocol_fee.safe_sub(host_fee)?;
    let amount_into_bin = amount_in_with_fees.safe_sub(fee)?;

    if swap_for_y {
        bin.amount_x = bin.amount_x.safe_add(amount_into_bin)?;
        bin.amount_y = bin.amount_y.safe_sub(amount_out)?;
    } else {
        bin.amount_y = bin.amount_y.safe_add(amount_into_bin)?;
        bin.amount_x = bin.amount_x.safe_sub(amount_out)?;
    }

    Ok(BinSwapResult {
        amount_in_with_fees,
        amount_out,
        fee,
        protocol_fee_after_host_fee,
        host_fee,
        is_exact_out_amount: false,
    })
}

/// Exact-out variant: swap until exactly `exact_out_amount` is
/// produced (or the bin is drained, whichever happens first).
pub fn bin_swap_exact_out(
    bin: &mut Bin,
    amount_in: u64,
    price: u128,
    swap_for_y: bool,
    pair: &LbPair,
    host_fee_bps: Option<u16>,
    exact_out_amount: u64,
) -> Result<BinSwapResult, &'static str> {
    let bin_max_amount_out = max_amount_out(bin, swap_for_y);
    if exact_out_amount >= bin_max_amount_out {
        let mut result = bin_swap(bin, amount_in, price, swap_for_y, pair, host_fee_bps)?;
        if exact_out_amount == bin_max_amount_out {
            result.is_exact_out_amount = true;
        }
        Ok(result)
    } else {
        let exact_amount_in = get_amount_in(exact_out_amount, price, swap_for_y)?;
        let fee = lb_math::compute_fee(pair, exact_amount_in)?;
        let amount_in_with_fees = exact_amount_in.safe_add(fee)?;
        let mut result = bin_swap(
            bin,
            amount_in_with_fees,
            price,
            swap_for_y,
            pair,
            host_fee_bps,
        )?;
        result.is_exact_out_amount = true;
        Ok(result)
    }
}
