//! Bitmap traversal for the bin-walk quoter.
//!
//! Two layers:
//!
//! * The inline `LbPair::bin_array_bitmap: [u64; 16]` covers bin
//!   array indices in `[-512, 511]` — that's `BIN_ARRAY_BITMAP_SIZE
//!   * 2` bits at 64 bits per word.
//! * The `BinArrayBitmapExtension` account adds 12 pages of `[u64; 8]`
//!   each (positive + negative), extending coverage to the full
//!   `[MIN_BIN_ID, MAX_BIN_ID]` bin space for sparse pools.
//!
//! The on-chain program walks both layers in turn when it needs to
//! find the next bin array with liquidity in a given direction. We
//! mirror that walk exactly.

use ruint::aliases::{U1024, U512};

use super::super::constants::{BIN_ARRAY_BITMAP_SIZE, EXTENSION_BIN_ARRAY_BITMAP_SIZE};
use super::super::state::{BinArrayBitmapExtension, LbPair};
use super::safe_math::SafeMath;

/// Inline bitmap covers indices in this range.
pub fn lb_pair_bitmap_range() -> (i32, i32) {
    (-BIN_ARRAY_BITMAP_SIZE, BIN_ARRAY_BITMAP_SIZE - 1)
}

/// True if `bin_array_index` is outside the inline bitmap's range
/// (and therefore needs the extension account).
pub fn is_overflow_default_bin_array_bitmap(bin_array_index: i32) -> bool {
    let (min_bitmap_id, max_bitmap_id) = lb_pair_bitmap_range();
    bin_array_index > max_bitmap_id || bin_array_index < min_bitmap_id
}

fn lb_pair_offset(bin_array_index: i32) -> usize {
    (bin_array_index + BIN_ARRAY_BITMAP_SIZE) as usize
}

/// Find the next bin-array index (in the requested swap direction)
/// that's flagged in the *inline* bitmap. Returns
/// `(index, found)` — `found = false` means the search reached the
/// edge of the inline range without hitting liquidity.
pub fn next_bin_array_index_with_liquidity_internal(
    pair: &LbPair,
    swap_for_y: bool,
    start_array_index: i32,
) -> Result<(i32, bool), &'static str> {
    let bitmap = U1024::from_limbs(pair.bin_array_bitmap);
    let array_offset = lb_pair_offset(start_array_index);
    let (min_bitmap_id, max_bitmap_id) = lb_pair_bitmap_range();
    if swap_for_y {
        let bin_map_range: usize = max_bitmap_id
            .safe_sub(min_bitmap_id)?
            .try_into()
            .map_err(|_| "LBError::TypeCastFailed")?;
        let offset_bit_map = bitmap << bin_map_range.safe_sub(array_offset)?;
        if offset_bit_map.eq(&U1024::ZERO) {
            Ok((min_bitmap_id.safe_sub(1)?, false))
        } else {
            let next_bit = offset_bit_map.leading_zeros();
            Ok((start_array_index.safe_sub(next_bit as i32)?, true))
        }
    } else {
        let offset_bit_map = bitmap >> array_offset;
        if offset_bit_map.eq(&U1024::ZERO) {
            Ok((max_bitmap_id.safe_add(1)?, false))
        } else {
            let next_bit = offset_bit_map.trailing_zeros();
            Ok((
                start_array_index
                    .checked_add(next_bit as i32)
                    .ok_or("LBError::MathOverflow")?,
                true,
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// BinArrayBitmapExtension helpers
// ---------------------------------------------------------------------------

fn ext_bitmap_offset(bin_array_index: i32) -> usize {
    let offset = if bin_array_index > 0 {
        bin_array_index / BIN_ARRAY_BITMAP_SIZE - 1
    } else {
        -(bin_array_index + 1) / BIN_ARRAY_BITMAP_SIZE - 1
    };
    offset as usize
}

fn ext_bin_array_offset_in_bitmap(bin_array_index: i32) -> Result<usize, &'static str> {
    if bin_array_index > 0 {
        Ok(bin_array_index.safe_rem(BIN_ARRAY_BITMAP_SIZE)? as usize)
    } else {
        Ok((-(bin_array_index + 1)).safe_rem(BIN_ARRAY_BITMAP_SIZE)? as usize)
    }
}

fn ext_to_bin_array_index(
    offset: usize,
    bin_array_offset: usize,
    is_positive: bool,
) -> Result<i32, &'static str> {
    let offset = offset as i32;
    let bin_array_offset = bin_array_offset as i32;
    if is_positive {
        Ok((offset + 1) * BIN_ARRAY_BITMAP_SIZE + bin_array_offset)
    } else {
        Ok(-((offset + 1) * BIN_ARRAY_BITMAP_SIZE + bin_array_offset) - 1)
    }
}

fn ext_bitmap_range() -> (i32, i32) {
    (
        -BIN_ARRAY_BITMAP_SIZE * (EXTENSION_BIN_ARRAY_BITMAP_SIZE as i32 + 1),
        BIN_ARRAY_BITMAP_SIZE * (EXTENSION_BIN_ARRAY_BITMAP_SIZE as i32 + 1) - 1,
    )
}

/// Walk the extension's bitmap pages between `start_index` and
/// `end_index` (inclusive of start, direction-dependent end). Returns
/// the first bin-array index with liquidity, or `None` if the search
/// reached the edge.
pub fn ext_iter_bitmap(
    ext: &BinArrayBitmapExtension,
    start_index: i32,
    end_index: i32,
) -> Result<Option<i32>, &'static str> {
    let offset = ext_bitmap_offset(start_index);
    let bin_array_offset = ext_bin_array_offset_in_bitmap(start_index)?;
    if start_index < 0 {
        if start_index <= end_index {
            // Walk towards 0 (less negative).
            for i in (0..=offset).rev() {
                let mut bm = U512::from_limbs(ext.negative_bin_array_bitmap[i]);
                if i == offset {
                    bm <<= BIN_ARRAY_BITMAP_SIZE as usize - bin_array_offset - 1;
                    if bm.eq(&U512::ZERO) {
                        continue;
                    }
                    let off = bin_array_offset - bm.leading_zeros();
                    return Ok(Some(ext_to_bin_array_index(i, off, false)?));
                }
                if bm.eq(&U512::ZERO) {
                    continue;
                }
                let off = BIN_ARRAY_BITMAP_SIZE as usize - bm.leading_zeros() - 1;
                return Ok(Some(ext_to_bin_array_index(i, off, false)?));
            }
        } else {
            // Walk further negative.
            for i in offset..EXTENSION_BIN_ARRAY_BITMAP_SIZE {
                let mut bm = U512::from_limbs(ext.negative_bin_array_bitmap[i]);
                if i == offset {
                    bm >>= bin_array_offset;
                    if bm.eq(&U512::ZERO) {
                        continue;
                    }
                    let off = bin_array_offset + bm.trailing_zeros();
                    return Ok(Some(ext_to_bin_array_index(i, off, false)?));
                }
                if bm.eq(&U512::ZERO) {
                    continue;
                }
                let off = bm.trailing_zeros();
                return Ok(Some(ext_to_bin_array_index(i, off, false)?));
            }
        }
    } else if start_index <= end_index {
        // Walk towards more positive.
        for i in offset..EXTENSION_BIN_ARRAY_BITMAP_SIZE {
            let mut bm = U512::from_limbs(ext.positive_bin_array_bitmap[i]);
            if i == offset {
                bm >>= bin_array_offset;
                if bm.eq(&U512::ZERO) {
                    continue;
                }
                let off = bin_array_offset + bm.trailing_zeros();
                return Ok(Some(ext_to_bin_array_index(i, off, true)?));
            }
            if bm.eq(&U512::ZERO) {
                continue;
            }
            let off = bm.trailing_zeros();
            return Ok(Some(ext_to_bin_array_index(i, off, true)?));
        }
    } else {
        // Walk towards 0 (less positive).
        for i in (0..=offset).rev() {
            let mut bm = U512::from_limbs(ext.positive_bin_array_bitmap[i]);
            if i == offset {
                bm <<= BIN_ARRAY_BITMAP_SIZE as usize - bin_array_offset - 1;
                if bm.eq(&U512::ZERO) {
                    continue;
                }
                let off = bin_array_offset - bm.leading_zeros();
                return Ok(Some(ext_to_bin_array_index(i, off, true)?));
            }
            if bm.eq(&U512::ZERO) {
                continue;
            }
            let off = BIN_ARRAY_BITMAP_SIZE as usize - bm.leading_zeros() - 1;
            return Ok(Some(ext_to_bin_array_index(i, off, true)?));
        }
    }
    Ok(None)
}

/// Find the next bin-array index in the extension layer.
pub fn next_bin_array_index_with_liquidity_extension(
    ext: &BinArrayBitmapExtension,
    swap_for_y: bool,
    start_index: i32,
) -> Result<(i32, bool), &'static str> {
    let (min_bitmap_id, max_bit_map_id) = ext_bitmap_range();
    if start_index > 0 {
        if swap_for_y {
            match ext_iter_bitmap(ext, start_index, BIN_ARRAY_BITMAP_SIZE)? {
                Some(v) => Ok((v, true)),
                None => Ok((BIN_ARRAY_BITMAP_SIZE - 1, false)),
            }
        } else {
            match ext_iter_bitmap(ext, start_index, max_bit_map_id)? {
                Some(v) => Ok((v, true)),
                None => Err("LBError::CannotFindNonZeroLiquidityBinArrayId"),
            }
        }
    } else if swap_for_y {
        match ext_iter_bitmap(ext, start_index, min_bitmap_id)? {
            Some(v) => Ok((v, true)),
            None => Err("LBError::CannotFindNonZeroLiquidityBinArrayId"),
        }
    } else {
        match ext_iter_bitmap(ext, start_index, -BIN_ARRAY_BITMAP_SIZE - 1)? {
            Some(v) => Ok((v, true)),
            None => Ok((-BIN_ARRAY_BITMAP_SIZE, false)),
        }
    }
}

/// Move `pair.active_id` to the appropriate end of `bin_array_index`'s
/// range based on swap direction. Used after the bitmap walk finds a
/// new array — we jump to its boundary, then the bin walk continues.
pub fn shift_active_bin(
    pair: &mut LbPair,
    swap_for_y: bool,
    bin_array_index: i32,
) -> Result<(), &'static str> {
    use super::bin_array::bin_array_lower_upper_bin_id;
    let (lower, upper) = bin_array_lower_upper_bin_id(bin_array_index)?;
    pair.active_id = if swap_for_y { upper } else { lower };
    Ok(())
}

/// Top-level: advance `pair.active_id` to the next bin-array with
/// liquidity in the swap direction. Walks the inline bitmap first;
/// falls through to the extension when needed. Errors if liquidity
/// isn't found anywhere in the swap direction.
pub fn advance_to_next_liquid_bin_array(
    pair: &mut LbPair,
    swap_for_y: bool,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
) -> Result<(), &'static str> {
    use super::bin_array::bin_id_to_bin_array_index;
    let start_array_index = bin_id_to_bin_array_index(pair.active_id)?;
    if is_overflow_default_bin_array_bitmap(start_array_index) {
        let ext = bitmap_extension.ok_or("LBError::BitmapExtensionAccountIsNotProvided")?;
        let (idx, found) =
            next_bin_array_index_with_liquidity_extension(ext, swap_for_y, start_array_index)?;
        if found {
            if start_array_index != idx {
                shift_active_bin(pair, swap_for_y, idx)?;
            }
        } else {
            // Walked off the extension's edge; fall back to the
            // inline bitmap on the other side.
            advance_internal_to_extension(
                pair,
                swap_for_y,
                start_array_index,
                idx,
                bitmap_extension,
            )?;
        }
    } else {
        advance_internal_to_extension(
            pair,
            swap_for_y,
            start_array_index,
            start_array_index,
            bitmap_extension,
        )?;
    }
    Ok(())
}

fn advance_internal_to_extension(
    pair: &mut LbPair,
    swap_for_y: bool,
    current_array_index: i32,
    start_array_index: i32,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
) -> Result<(), &'static str> {
    let (idx, found) =
        next_bin_array_index_with_liquidity_internal(pair, swap_for_y, start_array_index)?;
    if found {
        if current_array_index != idx {
            shift_active_bin(pair, swap_for_y, idx)?;
        }
    } else {
        let ext = bitmap_extension.ok_or("LBError::BitmapExtensionAccountIsNotProvided")?;
        let (idx, _) = next_bin_array_index_with_liquidity_extension(ext, swap_for_y, idx)?;
        if current_array_index != idx {
            shift_active_bin(pair, swap_for_y, idx)?;
        }
    }
    Ok(())
}
