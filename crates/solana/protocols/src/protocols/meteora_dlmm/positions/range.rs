//! Liquidity-distribution helpers for `add_liquidity*` ixs.
//!
//! The on-chain `add_liquidity` family has three argument shapes:
//!
//! * [`LiquidityParameter`] — explicit per-bin distribution (`Vec<BinLiquidityDistribution>`).
//!   Each entry says "deposit this fraction of amount_x and this
//!   fraction of amount_y into this bin id." Distribution units are
//!   basis-points-of-`BASIS_POINT_MAX` (i.e. parts-per-10000).
//! * [`LiquidityParameterByStrategy`] — opaque [`StrategyType`] + `[u8; 64]`
//!   parameter bag. The on-chain program expands the strategy into
//!   per-bin amounts itself; we just pick a strategy variant and
//!   pass `(amount_x, amount_y, active_id, max_slippage)`.
//! * `LiquidityParameterByWeight` — explicit per-bin *weight*
//!   (single `u16` per bin); the program normalises and splits
//!   `(amount_x, amount_y)` proportionally across them.
//!
//! This module exposes builders for the *explicit-distribution*
//! shape ([`LiquidityParameter`]) for the simplest cases (Spot,
//! one-sided), and helpers for the strategy-payload shape that
//! the SDK already supports first-class.
//!
//! Naming: distribution sums to `BASIS_POINT_MAX = 10_000`. Spot =
//! uniform across the range; one-sided = all on a single side.
//!
//! [`LiquidityParameter`]: meteora_dlmm_sdk::types::LiquidityParameter
//! [`LiquidityParameterByStrategy`]: meteora_dlmm_sdk::types::LiquidityParameterByStrategy
//! [`StrategyType`]: meteora_dlmm_sdk::types::StrategyType

use meteora_dlmm_sdk::types::{
    BinLiquidityDistribution, LiquidityParameter, LiquidityParameterByStrategy, StrategyParameters,
    StrategyType,
};

use crate::protocols::meteora_dlmm::constants::{BASIS_POINT_MAX, MAX_BIN_PER_ARRAY};

// ---------------------------------------------------------------------------
// Strategy payload helpers (StrategyType-driven, single ix arg)
// ---------------------------------------------------------------------------

/// Side of an asymmetric distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategySide {
    /// Token X only (e.g. memecoin in a SOL/MEME pair). Range sits
    /// strictly *above* the active bin.
    XOnly,
    /// Token Y only (e.g. SOL in a SOL/MEME pair). Range sits
    /// strictly *below* the active bin.
    YOnly,
    /// Both sides; active bin is somewhere inside the range.
    Both,
}

/// One of the SDK's nine [`StrategyType`] variants, lifted to a
/// shape the lp-position layer can pass without naming the SDK
/// enum at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeStrategy {
    /// Uniform liquidity across the range.
    Spot(BalancedShape),
    /// Higher liquidity at the centre of the range.
    Curve(BalancedShape),
    /// Higher liquidity at the edges (bid + ask shape).
    BidAsk(BalancedShape),
}

/// Whether a strategy's range sits one-sided or straddles the
/// active bin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalancedShape {
    OneSide(StrategySide),
    Balanced,
    Imbalanced,
}

impl RangeStrategy {
    fn to_sdk(self) -> StrategyType {
        match self {
            RangeStrategy::Spot(BalancedShape::OneSide(_)) => StrategyType::SpotOneSide,
            RangeStrategy::Spot(BalancedShape::Balanced) => StrategyType::SpotBalanced,
            RangeStrategy::Spot(BalancedShape::Imbalanced) => StrategyType::SpotImBalanced,
            RangeStrategy::Curve(BalancedShape::OneSide(_)) => StrategyType::CurveOneSide,
            RangeStrategy::Curve(BalancedShape::Balanced) => StrategyType::CurveBalanced,
            RangeStrategy::Curve(BalancedShape::Imbalanced) => StrategyType::CurveImBalanced,
            RangeStrategy::BidAsk(BalancedShape::OneSide(_)) => StrategyType::BidAskOneSide,
            RangeStrategy::BidAsk(BalancedShape::Balanced) => StrategyType::BidAskBalanced,
            RangeStrategy::BidAsk(BalancedShape::Imbalanced) => StrategyType::BidAskImBalanced,
        }
    }
}

/// Build a [`LiquidityParameterByStrategy`] payload for an
/// `add_liquidity_by_strategy` / `add_liquidity_by_strategy2` ix.
///
/// `max_active_bin_slippage` is the +/- bin tolerance — the on-chain
/// program rejects the deposit if the active bin has moved by more
/// than this between the user observing it (`active_id`) and the ix
/// landing.
///
/// Errors when the range exceeds [`MAX_BIN_PER_ARRAY * 2`] (the
/// position's `liquidity_shares\[70\]` slot count after accounting for
/// the on-chain layout).
pub fn strategy_payload(
    amount_x: u64,
    amount_y: u64,
    active_id: i32,
    min_bin_id: i32,
    max_bin_id: i32,
    strategy: RangeStrategy,
    max_active_bin_slippage: i32,
) -> Result<LiquidityParameterByStrategy, &'static str> {
    if max_bin_id < min_bin_id {
        return Err("LBError::InvalidBinId");
    }
    let width = (max_bin_id - min_bin_id + 1) as usize;
    if width > MAX_BIN_PER_ARRAY {
        return Err("range exceeds 70 bins (positions cap at 70 liquidity slots)");
    }
    Ok(LiquidityParameterByStrategy {
        amount_x,
        amount_y,
        active_id,
        max_active_bin_slippage,
        strategy_parameters: StrategyParameters {
            min_bin_id,
            max_bin_id,
            strategy_type: strategy.to_sdk(),
            // The on-chain program ignores `parameteres` for the
            // built-in strategies; preset to zeros.
            parameteres: [0u8; 64],
        },
    })
}

// ---------------------------------------------------------------------------
// Explicit-distribution helpers (LiquidityParameter, basis points)
// ---------------------------------------------------------------------------

/// Distribution shape for the explicit `LiquidityParameter` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneSideShape {
    /// All liquidity on the X side; range sits strictly above the
    /// active bin.
    XAboveActive,
    /// All liquidity on the Y side; range sits strictly below the
    /// active bin.
    YBelowActive,
}

/// Spot (uniform) one-sided deposit. Splits `BASIS_POINT_MAX` evenly
/// across `[lower_bin_id, upper_bin_id]` on the requested side.
///
/// Use when you know the exact amount of one token to deposit and
/// want a uniform shape. For asymmetric / both-sided deposits, use
/// [`spot_balanced`] or [`strategy_payload`] instead.
pub fn spot_one_side(
    amount_x: u64,
    amount_y: u64,
    lower_bin_id: i32,
    upper_bin_id: i32,
    side: OneSideShape,
) -> Result<LiquidityParameter, &'static str> {
    let bins = build_uniform_dist(lower_bin_id, upper_bin_id, side_to_uniform(side))?;
    Ok(LiquidityParameter {
        amount_x,
        amount_y,
        bin_liquidity_dist: bins,
    })
}

/// Spot (uniform) balanced deposit. Active bin must be inside the
/// range. X liquidity goes to bins above active; Y liquidity goes to
/// bins below; the active bin gets a half-share of each.
///
/// This is the simplest "I want a passive LP across a range
/// straddling spot" shape — most MM use cases will reach for
/// [`strategy_payload`] with a `Spot(Balanced)` strategy instead.
pub fn spot_balanced(
    amount_x: u64,
    amount_y: u64,
    active_id: i32,
    lower_bin_id: i32,
    upper_bin_id: i32,
) -> Result<LiquidityParameter, &'static str> {
    if active_id < lower_bin_id || active_id > upper_bin_id {
        return Err("active_id outside range — use spot_one_side");
    }
    let width = (upper_bin_id - lower_bin_id + 1) as usize;
    if width == 0 || width > MAX_BIN_PER_ARRAY {
        return Err("range invalid (must be 1..=70 bins)");
    }
    let per_bin = (BASIS_POINT_MAX as u32 / width as u32) as u16;
    let mut remainder_x = BASIS_POINT_MAX;
    let mut remainder_y = BASIS_POINT_MAX;
    let mut bins = Vec::with_capacity(width);
    for bin_id in lower_bin_id..=upper_bin_id {
        // X sits at and above active; Y sits at and below active.
        // The active bin itself gets both sides.
        let dx = if bin_id >= active_id {
            std::cmp::min(per_bin as i32, remainder_x).max(0) as u16
        } else {
            0
        };
        let dy = if bin_id <= active_id {
            std::cmp::min(per_bin as i32, remainder_y).max(0) as u16
        } else {
            0
        };
        remainder_x -= dx as i32;
        remainder_y -= dy as i32;
        bins.push(BinLiquidityDistribution {
            bin_id,
            distribution_x: dx,
            distribution_y: dy,
        });
    }
    // Push remainders onto the last X bin / first Y bin so the
    // sums exactly equal BASIS_POINT_MAX.
    if remainder_x > 0 {
        if let Some(last) = bins.iter_mut().rev().find(|b| b.bin_id >= active_id) {
            last.distribution_x = last.distribution_x.saturating_add(remainder_x as u16);
        }
    }
    if remainder_y > 0 {
        if let Some(first) = bins.iter_mut().find(|b| b.bin_id <= active_id) {
            first.distribution_y = first.distribution_y.saturating_add(remainder_y as u16);
        }
    }
    Ok(LiquidityParameter {
        amount_x,
        amount_y,
        bin_liquidity_dist: bins,
    })
}

fn side_to_uniform(side: OneSideShape) -> UniformSide {
    match side {
        OneSideShape::XAboveActive => UniformSide::X,
        OneSideShape::YBelowActive => UniformSide::Y,
    }
}

#[derive(Debug, Clone, Copy)]
enum UniformSide {
    X,
    Y,
}

fn build_uniform_dist(
    lower: i32,
    upper: i32,
    side: UniformSide,
) -> Result<Vec<BinLiquidityDistribution>, &'static str> {
    if upper < lower {
        return Err("LBError::InvalidBinId");
    }
    let width = (upper - lower + 1) as usize;
    if width == 0 || width > MAX_BIN_PER_ARRAY {
        return Err("range invalid (must be 1..=70 bins)");
    }
    let per_bin = (BASIS_POINT_MAX as u32 / width as u32) as u16;
    let mut remainder = BASIS_POINT_MAX;
    let mut out = Vec::with_capacity(width);
    for bin_id in lower..=upper {
        let chunk = std::cmp::min(per_bin as i32, remainder).max(0) as u16;
        remainder -= chunk as i32;
        let (dx, dy) = match side {
            UniformSide::X => (chunk, 0),
            UniformSide::Y => (0, chunk),
        };
        out.push(BinLiquidityDistribution {
            bin_id,
            distribution_x: dx,
            distribution_y: dy,
        });
    }
    // Remainder onto the last bin so the sum is exact.
    if remainder > 0 {
        if let Some(last) = out.last_mut() {
            match side {
                UniformSide::X => {
                    last.distribution_x = last.distribution_x.saturating_add(remainder as u16);
                }
                UniformSide::Y => {
                    last.distribution_y = last.distribution_y.saturating_add(remainder as u16);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_one_side_x_distribution_sums_to_basis_point_max() {
        let p = spot_one_side(1_000_000, 0, 0, 9, OneSideShape::XAboveActive).unwrap();
        assert_eq!(p.bin_liquidity_dist.len(), 10);
        let sum_x: u32 = p
            .bin_liquidity_dist
            .iter()
            .map(|b| u32::from(b.distribution_x))
            .sum();
        let sum_y: u32 = p
            .bin_liquidity_dist
            .iter()
            .map(|b| u32::from(b.distribution_y))
            .sum();
        assert_eq!(sum_x, BASIS_POINT_MAX as u32);
        assert_eq!(sum_y, 0);
    }

    #[test]
    fn spot_balanced_x_above_y_below_active() {
        let p = spot_balanced(1_000_000, 1_000_000, 5, 0, 10).unwrap();
        assert_eq!(p.bin_liquidity_dist.len(), 11);
        let sum_x: u32 = p
            .bin_liquidity_dist
            .iter()
            .map(|b| u32::from(b.distribution_x))
            .sum();
        let sum_y: u32 = p
            .bin_liquidity_dist
            .iter()
            .map(|b| u32::from(b.distribution_y))
            .sum();
        assert_eq!(sum_x, BASIS_POINT_MAX as u32);
        assert_eq!(sum_y, BASIS_POINT_MAX as u32);
        // Bin 0 should have only Y (below active = 5).
        let b0 = p.bin_liquidity_dist.iter().find(|b| b.bin_id == 0).unwrap();
        assert_eq!(b0.distribution_x, 0);
        assert!(b0.distribution_y > 0);
        // Bin 10 should have only X (above active).
        let b10 = p
            .bin_liquidity_dist
            .iter()
            .find(|b| b.bin_id == 10)
            .unwrap();
        assert!(b10.distribution_x > 0);
        assert_eq!(b10.distribution_y, 0);
    }

    #[test]
    fn strategy_payload_too_wide_errors() {
        let result = strategy_payload(
            100,
            100,
            0,
            -50,
            50,
            RangeStrategy::Spot(BalancedShape::Balanced),
            5,
        );
        // 101 bins > 70, should fail.
        assert!(result.is_err());
    }

    #[test]
    fn strategy_payload_emits_correct_strategy_type() {
        let p = strategy_payload(
            100,
            100,
            0,
            -10,
            10,
            RangeStrategy::Curve(BalancedShape::Balanced),
            5,
        )
        .unwrap();
        assert_eq!(
            p.strategy_parameters.strategy_type,
            StrategyType::CurveBalanced
        );
        assert_eq!(p.strategy_parameters.min_bin_id, -10);
        assert_eq!(p.strategy_parameters.max_bin_id, 10);
    }

    #[test]
    fn balanced_shape_active_outside_range_errors() {
        assert!(spot_balanced(100, 100, -5, 0, 10).is_err());
    }
}
