//! Helpers around `BinArray` indexing — converting between bin ids
//! and bin-array indices, and pulling individual `Bin`s out of an
//! array. Free functions on the SDK's [`BinArray`] type.

use num_integer::Integer;

use super::super::constants::{MAX_BIN_ID, MAX_BIN_PER_ARRAY, MIN_BIN_ID};
use super::super::state::{Bin, BinArray};
use super::safe_math::SafeMath;

/// Compute which bin-array index contains a given bin id.
///
/// `MAX_BIN_PER_ARRAY = 70`. For positive bin ids this is plain
/// integer division; for negative bin ids the remainder pushes the
/// index *down* by one (so bin `-1` lives in array `-1`, not `0`).
pub fn bin_id_to_bin_array_index(bin_id: i32) -> Result<i32, &'static str> {
    let (idx, rem) = bin_id.div_rem(&(MAX_BIN_PER_ARRAY as i32));
    if bin_id.is_negative() && rem != 0 {
        idx.safe_sub(1)
    } else {
        Ok(idx)
    }
}

/// Lower / upper bin id span for a given bin-array index.
pub fn bin_array_lower_upper_bin_id(index: i32) -> Result<(i32, i32), &'static str> {
    let lower = index.safe_mul(MAX_BIN_PER_ARRAY as i32)?;
    let upper = lower.safe_add(MAX_BIN_PER_ARRAY as i32)?.safe_sub(1)?;
    Ok((lower, upper))
}

/// Validate that `index` corresponds to a bin range entirely within
/// `[MIN_BIN_ID, MAX_BIN_ID]`.
pub fn check_valid_index(index: i32) -> Result<(), &'static str> {
    let (lower, upper) = bin_array_lower_upper_bin_id(index)?;
    if lower >= MIN_BIN_ID && upper <= MAX_BIN_ID {
        Ok(())
    } else {
        Err("LBError::InvalidStartBinIndex")
    }
}

/// Resolve `bin_id` to its index inside this `BinArray`'s `bins[]`
/// slice. Errors if `bin_id` is outside the array's covered range.
fn bin_index_in_array(array: &BinArray, bin_id: i32) -> Result<usize, &'static str> {
    let array_index_i32 = i32::try_from(array.index).map_err(|_| "LBError::TypeCastFailed")?;
    let (lower, upper) = bin_array_lower_upper_bin_id(array_index_i32)?;
    if bin_id < lower || bin_id > upper {
        return Err("LBError::InvalidBinId");
    }
    let index = if bin_id.is_positive() {
        bin_id.safe_sub(lower)?
    } else {
        ((MAX_BIN_PER_ARRAY as i32).safe_sub(upper.safe_sub(bin_id)?)?).safe_sub(1)?
    };
    if (0..(MAX_BIN_PER_ARRAY as i32)).contains(&index) {
        Ok(index as usize)
    } else {
        Err("LBError::InvalidBinId")
    }
}

/// Return `true` iff `bin_id` lies inside this array's range.
pub fn is_bin_id_within_range(array: &BinArray, bin_id: i32) -> bool {
    let array_index_i32 = match i32::try_from(array.index) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let (lower, upper) = match bin_array_lower_upper_bin_id(array_index_i32) {
        Ok(v) => v,
        Err(_) => return false,
    };
    bin_id >= lower && bin_id <= upper
}

/// Read-only access to a bin within an array.
pub fn get_bin(array: &BinArray, bin_id: i32) -> Result<&Bin, &'static str> {
    let idx = bin_index_in_array(array, bin_id)?;
    Ok(&array.bins[idx])
}

/// Mutable access to a bin within an array.
pub fn get_bin_mut(array: &mut BinArray, bin_id: i32) -> Result<&mut Bin, &'static str> {
    let idx = bin_index_in_array(array, bin_id)?;
    Ok(&mut array.bins[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_bin_id_to_array_index() {
        // Array 0 covers bins 0..=69; bin 70 starts array 1.
        assert_eq!(bin_id_to_bin_array_index(0).unwrap(), 0);
        assert_eq!(bin_id_to_bin_array_index(69).unwrap(), 0);
        assert_eq!(bin_id_to_bin_array_index(70).unwrap(), 1);
    }

    #[test]
    fn negative_bin_id_to_array_index() {
        // Array -1 covers bins -70..=-1; bin -71 starts array -2.
        assert_eq!(bin_id_to_bin_array_index(-1).unwrap(), -1);
        assert_eq!(bin_id_to_bin_array_index(-70).unwrap(), -1);
        assert_eq!(bin_id_to_bin_array_index(-71).unwrap(), -2);
    }

    #[test]
    fn array_lower_upper_round_trips() {
        for idx in [-3i32, -1, 0, 5, 100] {
            let (lower, upper) = bin_array_lower_upper_bin_id(idx).unwrap();
            assert_eq!(upper - lower + 1, MAX_BIN_PER_ARRAY as i32);
            assert_eq!(bin_id_to_bin_array_index(lower).unwrap(), idx);
            assert_eq!(bin_id_to_bin_array_index(upper).unwrap(), idx);
        }
    }
}
