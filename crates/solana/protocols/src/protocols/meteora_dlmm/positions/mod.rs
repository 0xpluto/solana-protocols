//! DLMM-specific position math + helpers.
//!
//! Lives next to `math/` (swap-side math) but kept separate because
//! the surface is different — these helpers operate on a
//! [`PositionV2`] anchored to a specific [`LbPair`] / set of
//! [`BinArray`]s. The generic position-management layer
//! (`crates/trading/lp-position`) consumes this module via
//! re-exports; it never reaches into bin-level math directly.
//!
//! Three pieces:
//!
//! * [`view`] — derived state that depends on current bin prices
//!   (current value, uncollected fees, in-range flag, range width).
//!   Joins a [`PositionV2`] to a [`LbPair`] + [`BinArray`] map from
//!   the cache.
//! * [`coverage`] — which bin-array PDAs the position spans. The
//!   position covers `[lower_bin_id, upper_bin_id]`; this resolves
//!   that range to the 1–2 bin-array PDAs you need to load /
//!   include as `remaining_accounts` for any add/remove/claim ix.
//! * [`range`] — Spot / Curve / BidAsk distribution helpers that
//!   produce [`LiquidityParameter`] / [`LiquidityParameterByStrategy`]
//!   payloads for `add_liquidity*` ixs.
//!
//! [`PositionV2`]: crate::protocols::meteora_dlmm::PositionV2
//! [`LbPair`]: crate::protocols::meteora_dlmm::LbPair
//! [`BinArray`]: crate::protocols::meteora_dlmm::BinArray
//! [`LiquidityParameter`]: meteora_dlmm_sdk::types::LiquidityParameter
//! [`LiquidityParameterByStrategy`]: meteora_dlmm_sdk::types::LiquidityParameterByStrategy

pub mod coverage;
pub mod range;
pub mod view;

pub use coverage::{bin_array_keys_for_position, bin_array_keys_for_range};
pub use range::{
    spot_balanced, spot_one_side, BalancedShape, OneSideShape, RangeStrategy, StrategySide,
};
pub use view::{compute_metrics, PositionMetrics, PositionRangeStatus};
