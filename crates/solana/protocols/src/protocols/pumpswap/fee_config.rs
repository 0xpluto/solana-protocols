//! PumpSwap's dynamic fee config.
//!
//! The `pump_fees` program serves **both** pumpfun and PumpSwap: each consuming
//! program gets its own `FeeConfig` PDA (seeds `["fee_config", <consuming
//! program id>]`), but they share an owner, an `account:FeeConfig`
//! discriminator, and a byte layout — verified on mainnet 2026-08-09:
//!
//! | PDA | owner | disc | size |
//! |---|---|---|---|
//! | `8Wf5Ti…` (pumpfun) | `pfeeUxB…` | `[143,52,146,…]` | 4073 |
//! | `5PHirr…` (PumpSwap) | `pfeeUxB…` | `[143,52,146,…]` | 4073 |
//!
//! That identity is exactly why they collide in a discriminator-keyed registry:
//! one handler matches both, and without pubkey routing the second update
//! overwrites the first (see [`PumpfunFeeConfigHandler`]). The layout being
//! shared is what lets one decoder serve both; this **newtype** is what keeps
//! them in separate cache slots so neither can clobber the other.
//!
//! [`PumpfunFeeConfigHandler`]: super::super::pumpfun::handler::PumpfunFeeConfigHandler

use super::super::pumpfun::PumpfunFeeConfig;

/// PumpSwap's `FeeConfig` PDA contents — the same `pump_fees` layout as
/// [`PumpfunFeeConfig`], newtyped so the cache holds it in its own singleton
/// slot rather than overwriting pumpfun's.
///
/// Fee lookup is the same operation on the inner value (market-cap-keyed tiers
/// via [`calculate_fee_tier`](super::super::pumpfun::calculate_fee_tier)).
#[derive(Clone, Debug)]
pub struct PumpSwapFeeConfig(pub PumpfunFeeConfig);

impl PumpSwapFeeConfig {
    /// The underlying fee config.
    #[must_use]
    pub fn inner(&self) -> &PumpfunFeeConfig {
        &self.0
    }
}
