//! Protocol implementations.
//!
//! This module contains the [`Protocol`] enum, unified pool types, and
//! all protocol-specific implementations. The design enforces exhaustive
//! matching - adding a new variant to [`Protocol`] will cause compiler
//! errors everywhere that needs protocol-specific logic.
//!
//! # Core Types
//!
//! - [`Protocol`] - Enum of all supported protocols
//! - [`PoolState`] - Unified pool state wrapping protocol-specific states
//! - [`PoolKeys`] - Unified pool keys wrapping protocol-specific keys
//!
//! # Adding a New Protocol
//!
//! 1. Add variant to [`Protocol`] enum
//! 2. Add variant to [`PoolState`] enum
//! 3. Add variant to [`PoolKeys`] enum
//! 4. `cargo build` will show all locations needing updates
//! 5. Create new protocol module: `protocols/new_protocol/`
//! 6. Implement required traits for the protocol
//!
//! The compiler will guide you through every location that needs
//! protocol-specific logic. No wildcards are used, ensuring complete
//! implementation.
//!
//! # Traits to Implement
//!
//! Each protocol's state type must implement:
//! - [`SwapMath`] - Calculate swap outputs from pool state
//! - `from_account_data(&[u8])` - Parse from raw bytes (via `#[derive(OnchainState)]`)
//!
//! Each protocol's instruction builder must implement:
//! - `InstructionBuilder` - Build Solana instructions

pub mod option_bool;
pub use option_bool::{OptionBool, UnknownOptionBool};

pub mod pumpfun;
pub mod pumpswap;
pub mod raydium_v4;
pub mod spl_token;

// Protocol stubs — instruction parsing added in subsequent phases
pub mod meteora_damm_v2;
pub mod meteora_dbc;
pub mod meteora_dlmm;
pub mod raydium_clmm;
pub mod raydium_cpmm;
pub mod raydium_launchpad;

use solana_program::pubkey::Pubkey;

// Bring DLMM extension methods into scope so `pool.spot_price()` etc.
// resolve on the SDK's bare `LbPair` struct.
#[allow(unused_imports)]
use meteora_dlmm::LbPairExt as _;

use crate::events::SwapOutput;
use crate::traits::{SwapMath, SwapParams};
use crate::Result;

/// All supported protocols.
///
/// **IMPORTANT**: This enum uses exhaustive matching everywhere.
/// Adding a new variant will cause compiler errors at every location
/// that needs protocol-specific logic. This is intentional - it ensures
/// complete implementation when adding new protocols.
///
/// # Adding a New Protocol
///
/// 1. Add the variant here
/// 2. Add variants to [`PoolState`] and [`PoolKeys`]
/// 3. `cargo build` will show all locations needing updates
/// 4. Implement each match arm
/// 5. Create the protocol module with required trait impls
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Protocol {
    /// Pump.fun bonding curve protocol.
    Pumpfun,

    /// PumpSwap AMM (tokens graduate here from Pumpfun bonding curves).
    PumpSwap,

    /// Raydium V4 AMM (constant product).
    RaydiumV4,

    /// Raydium Concentrated Liquidity Market Maker.
    RaydiumClmm,

    /// Raydium Constant Product Market Maker.
    RaydiumCpmm,

    /// Raydium Launchpad (bonding curve with graduation).
    RaydiumLaunchpad,

    /// Meteora Dynamic Liquidity Market Maker (bin-based).
    MeteoraDlmm,

    /// Meteora Dynamic Bonding Curve.
    MeteoraDbC,

    /// Meteora DAMM v2 (cp-amm) concentrated-liquidity AMM.
    MeteoraDammV2,
}

// =============================================================================
// Unified Pool State
// =============================================================================

/// Unified pool state across all protocols.
///
/// **Superseded by [`QuoteState`](crate::quote::QuoteState).** This enum holds
/// one *account* per variant, which is not enough to price a swap on most
/// AMMs: PumpSwap keeps reserves in vault accounts and fee rates in a config
/// singleton, so `PoolState::PumpSwap` needs `PoolWithReserves` bolted on and
/// still quotes with a hardcoded fee ladder. `QuoteState` holds assembled
/// *bundles* instead, generated from one table.
///
/// It survives only because the legacy RPC assembly in `trading-execution`
/// constructs it for all nine protocols, and porting those is parked behind
/// pumpfun/pumpswap. It should not gain callers.
///
/// This enum wraps protocol-specific pool states, allowing unified handling
/// while preserving type safety. It implements [`SwapMath`] by dispatching
/// to the underlying protocol implementation.
///
/// # Adding a New Protocol
///
/// 1. Add a variant wrapping the protocol's state type
/// 2. Update all match arms (compiler will guide you)
///
/// # Example
///
/// ```ignore
/// let state = PoolState::from_account_data(Protocol::Pumpfun, &data)?;
/// let output = state.calculate_swap(&SwapParams::buy(1_000_000_000))?;
/// ```
// Variants wrap differently-sized per-protocol state; boxing would change the
// public type. Kept inline deliberately.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PoolState {
    /// Pump.fun bonding curve state.
    Pumpfun(pumpfun::BondingCurve),

    /// PumpSwap AMM state (constant product with inline reserves).
    PumpSwap(pumpswap::PoolWithReserves),

    /// Raydium V4 AMM state with reserves.
    /// NOTE: Raydium V4 stores reserves in vault accounts, not the pool state.
    /// Use `PoolWithReserves` for swap calculations.
    RaydiumV4(raydium_v4::PoolWithReserves),

    /// Raydium CLMM concentrated liquidity state.
    RaydiumClmm(raydium_clmm::PoolState),

    /// Raydium CPMM constant product state.
    /// NOTE: Reserves are in vault accounts, not inline.
    RaydiumCpmm(raydium_cpmm::CpmmPoolState),

    /// Raydium Launchpad bonding curve state.
    RaydiumLaunchpad(raydium_launchpad::LaunchpadPoolState),

    /// Meteora DLMM bin-based concentrated liquidity state.
    MeteoraDlmm(meteora_dlmm::LbPair),

    /// Meteora DBC virtual pool (bonding curve) state.
    MeteoraDbC(meteora_dbc::VirtualPool),
}

impl PoolState {
    /// Parse pool state from account data for a known protocol.
    ///
    /// This is the primary constructor - use when you know which protocol
    /// the account belongs to.
    ///
    /// NOTE: Raydium V4 requires vault reserves to be fetched separately.
    /// Use `from_raydium_v4_with_reserves()` instead.
    pub fn from_account_data(protocol: Protocol, data: &[u8]) -> Result<Self> {
        match protocol {
            Protocol::Pumpfun => {
                let curve = pumpfun::BondingCurve::from_account_data(data)?;
                Ok(PoolState::Pumpfun(curve))
            }
            Protocol::PumpSwap => {
                // Account data alone cannot supply reserves — they live in two
                // vault token accounts. Zeros here are explicit and visible:
                // `is_active()` reports false until a caller pairs the pool with
                // balances it actually read. (Raydium V4 below has the same
                // shape; both go away when PoolState becomes a quote bundle.)
                let pool = pumpswap::PumpSwapPool::from_account_data(data)?;
                Ok(PoolState::PumpSwap(pumpswap::PoolWithReserves::new(
                    pool, 0, 0,
                )))
            }
            Protocol::RaydiumV4 => {
                // Raydium V4 requires reserves from vault accounts.
                // Parse pool state but use zero reserves (caller must update).
                let pool = raydium_v4::LiquidityPool::from_account_data(data)?;
                let pool_with_reserves = raydium_v4::PoolWithReserves::new(pool, 0, 0);
                Ok(PoolState::RaydiumV4(pool_with_reserves))
            }
            Protocol::RaydiumClmm => {
                let pool = raydium_clmm::PoolState::from_account_data(data)
                    .map_err(crate::error::Error::parse_error)?;
                Ok(PoolState::RaydiumClmm(pool))
            }
            Protocol::RaydiumCpmm => {
                let pool = raydium_cpmm::CpmmPoolState::from_account_data(data)?;
                Ok(PoolState::RaydiumCpmm(pool))
            }
            Protocol::RaydiumLaunchpad => {
                let pool = raydium_launchpad::LaunchpadPoolState::from_account_data(data)?;
                Ok(PoolState::RaydiumLaunchpad(pool))
            }
            Protocol::MeteoraDlmm => {
                let pool = meteora_dlmm::decode_lb_pair(data)?;
                Ok(PoolState::MeteoraDlmm(pool))
            }
            Protocol::MeteoraDbC => {
                let pool = meteora_dbc::VirtualPool::from_account_data(data)?;
                Ok(PoolState::MeteoraDbC(pool))
            }
            Protocol::MeteoraDammV2 => {
                // Extraction-only today — no PoolState variant yet. Add
                // the struct + an arm here when we wire DAMM v2 trading.
                Err(crate::error::Error::parse_error(
                    "MeteoraDammV2 pool state parsing not implemented",
                ))
            }
        }
    }

    /// Create Raydium V4 pool state with reserves.
    ///
    /// Raydium V4 stores reserves in vault token accounts, not the pool state.
    /// Use this constructor when you have fetched the vault balances.
    pub fn from_raydium_v4_with_reserves(
        pool_data: &[u8],
        base_reserve: u64,
        quote_reserve: u64,
    ) -> Result<Self> {
        let pool = raydium_v4::LiquidityPool::from_account_data(pool_data)?;
        let pool_with_reserves =
            raydium_v4::PoolWithReserves::new(pool, base_reserve, quote_reserve);
        Ok(PoolState::RaydiumV4(pool_with_reserves))
    }

    /// Try to parse pool state by detecting the protocol from account data.
    ///
    /// Attempts each protocol's discriminator in order until one matches.
    /// Returns `None` if no protocol can parse the data.
    ///
    /// NOTE: Raydium V4 cannot be auto-detected because it has no discriminator.
    /// Use `from_account_data(Protocol::RaydiumV4, data)` explicitly.
    pub fn detect_and_parse(data: &[u8]) -> Option<(Protocol, Self)> {
        // Gate each parser on its 8-byte Anchor discriminator — the
        // parsers themselves are permissive about trailing bytes, so the
        // discriminator is the only reliable identity check.
        let disc = data.get(..8)?;

        if disc == pumpfun::BONDING_CURVE_DISCRIMINATOR {
            if let Ok(curve) = pumpfun::BondingCurve::from_account_data(data) {
                return Some((Protocol::Pumpfun, PoolState::Pumpfun(curve)));
            }
        }

        // PumpSwap can potentially be detected by discriminator
        if data.len() >= pumpswap::POOL_ACCOUNT_SIZE {
            if let Ok(pool) = pumpswap::PumpSwapPool::from_account_data(data) {
                // Detection only — reserves are not knowable from these bytes.
                return Some((
                    Protocol::PumpSwap,
                    PoolState::PumpSwap(pumpswap::PoolWithReserves::new(pool, 0, 0)),
                ));
            }
        }

        // Raydium V4 cannot be auto-detected (no discriminator)
        // Caller must use from_account_data with explicit protocol

        None
    }

    /// Get the protocol for this pool state.
    #[must_use]
    pub fn protocol(&self) -> Protocol {
        match self {
            PoolState::Pumpfun(_) => Protocol::Pumpfun,
            PoolState::PumpSwap(_) => Protocol::PumpSwap,
            PoolState::RaydiumV4(_) => Protocol::RaydiumV4,
            PoolState::RaydiumClmm(_) => Protocol::RaydiumClmm,
            PoolState::RaydiumCpmm(_) => Protocol::RaydiumCpmm,
            PoolState::RaydiumLaunchpad(_) => Protocol::RaydiumLaunchpad,
            PoolState::MeteoraDlmm(_) => Protocol::MeteoraDlmm,
            PoolState::MeteoraDbC(_) => Protocol::MeteoraDbC,
        }
    }

    /// Check if this pool is active and can execute swaps.
    #[must_use]
    pub fn is_active(&self) -> bool {
        match self {
            PoolState::Pumpfun(curve) => curve.is_active(),
            PoolState::PumpSwap(pool) => pool.is_active(),
            PoolState::RaydiumV4(pool) => pool.is_active(),
            PoolState::RaydiumClmm(pool) => pool.is_active(),
            PoolState::RaydiumCpmm(pool) => pool.is_active(),
            PoolState::RaydiumLaunchpad(pool) => pool.is_active(),
            PoolState::MeteoraDlmm(pool) => pool.is_active(),
            PoolState::MeteoraDbC(pool) => pool.is_active(),
        }
    }

    /// Get the current spot price.
    #[must_use]
    pub fn spot_price(&self) -> f64 {
        match self {
            PoolState::Pumpfun(curve) => curve.spot_price(),
            PoolState::PumpSwap(pool) => pool.spot_price(),
            PoolState::RaydiumV4(pool) => pool.spot_price(),
            PoolState::RaydiumClmm(pool) => pool.current_price(),
            PoolState::RaydiumCpmm(pool) => pool.spot_price(),
            PoolState::RaydiumLaunchpad(pool) => pool.spot_price(),
            PoolState::MeteoraDlmm(pool) => pool.spot_price(),
            PoolState::MeteoraDbC(pool) => pool.spot_price(),
        }
    }

    /// Unwrap as Pumpfun bonding curve.
    ///
    /// # Panics
    ///
    /// Panics if this is not a Pumpfun pool.
    #[must_use]
    pub fn as_pumpfun(&self) -> &pumpfun::BondingCurve {
        match self {
            PoolState::Pumpfun(curve) => curve,
            PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => panic!("not a Pumpfun pool"),
        }
    }

    /// Try to unwrap as Pumpfun bonding curve.
    #[must_use]
    pub fn try_as_pumpfun(&self) -> Option<&pumpfun::BondingCurve> {
        match self {
            PoolState::Pumpfun(curve) => Some(curve),
            PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as PumpSwap pool.
    ///
    /// # Panics
    ///
    /// Panics if this is not a PumpSwap pool.
    #[must_use]
    pub fn as_pumpswap(&self) -> &pumpswap::PoolWithReserves {
        match self {
            PoolState::PumpSwap(pool) => pool,
            PoolState::Pumpfun(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => panic!("not a PumpSwap pool"),
        }
    }

    /// Try to unwrap as PumpSwap pool.
    #[must_use]
    pub fn try_as_pumpswap(&self) -> Option<&pumpswap::PoolWithReserves> {
        match self {
            PoolState::PumpSwap(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Raydium V4 pool with reserves.
    ///
    /// # Panics
    ///
    /// Panics if this is not a Raydium V4 pool.
    #[must_use]
    pub fn as_raydium_v4(&self) -> &raydium_v4::PoolWithReserves {
        match self {
            PoolState::RaydiumV4(pool) => pool,
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => panic!("not a Raydium V4 pool"),
        }
    }

    /// Try to unwrap as Raydium V4 pool with reserves.
    #[must_use]
    pub fn try_as_raydium_v4(&self) -> Option<&raydium_v4::PoolWithReserves> {
        match self {
            PoolState::RaydiumV4(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Try to unwrap as Raydium CLMM pool.
    #[must_use]
    pub fn try_as_raydium_clmm(&self) -> Option<&raydium_clmm::PoolState> {
        match self {
            PoolState::RaydiumClmm(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Try to unwrap as Raydium CPMM pool.
    #[must_use]
    pub fn try_as_raydium_cpmm(&self) -> Option<&raydium_cpmm::CpmmPoolState> {
        match self {
            PoolState::RaydiumCpmm(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Try to unwrap as Raydium Launchpad pool.
    #[must_use]
    pub fn try_as_raydium_launchpad(&self) -> Option<&raydium_launchpad::LaunchpadPoolState> {
        match self {
            PoolState::RaydiumLaunchpad(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::MeteoraDlmm(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Try to unwrap as Meteora DLMM pair.
    #[must_use]
    pub fn try_as_meteora_dlmm(&self) -> Option<&meteora_dlmm::LbPair> {
        match self {
            PoolState::MeteoraDlmm(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDbC(_) => None,
        }
    }

    /// Try to unwrap as Meteora DBC virtual pool.
    #[must_use]
    pub fn try_as_meteora_dbc(&self) -> Option<&meteora_dbc::VirtualPool> {
        match self {
            PoolState::MeteoraDbC(pool) => Some(pool),
            PoolState::Pumpfun(_)
            | PoolState::PumpSwap(_)
            | PoolState::RaydiumV4(_)
            | PoolState::RaydiumClmm(_)
            | PoolState::RaydiumCpmm(_)
            | PoolState::RaydiumLaunchpad(_)
            | PoolState::MeteoraDlmm(_) => None,
        }
    }
}

impl SwapMath for PoolState {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        match self {
            PoolState::Pumpfun(curve) => curve.calculate_swap(params),
            PoolState::PumpSwap(pool) => pool.calculate_swap(params),
            PoolState::RaydiumV4(pool) => pool.calculate_swap(params),
            // New protocols: swap math requires complex state (tick arrays, bins, etc.)
            // Deferred to future phases — use spot_price() for pricing
            PoolState::RaydiumClmm(pool) => {
                // Single-range estimate (no tick crossing).
                // For exact results, use ClmmSwapComputer::compute_swap with tick arrays.
                let amount_in = params.amount.amount();
                let is_base_input = match params.direction {
                    crate::traits::SwapDirection::Buy => false, // SOL in → is quote input
                    crate::traits::SwapDirection::Sell => true, // token in → is base input
                };
                let fee_rate = 2500; // Default 0.25% — actual rate is in amm_config
                let (amount_out, fee) = raydium_clmm::ClmmSwapComputer::estimate_output(
                    pool.sqrt_price_x64,
                    pool.liquidity,
                    amount_in,
                    is_base_input,
                    fee_rate,
                );
                Ok(SwapOutput {
                    amount_in,
                    amount_out,
                    fee,
                    protocol_fee: 0,
                    lp_fee: fee,
                    price_impact_bps: 0,
                    effective_price: if amount_in > 0 {
                        amount_out as f64 / amount_in as f64
                    } else {
                        0.0
                    },
                    post_swap_price: pool.current_price(),
                    accounts_used: Vec::new(),
                })
            }
            PoolState::RaydiumCpmm(_pool) => {
                // CPMM requires vault reserves from separate accounts.
                // Without reserves, we can't calculate swaps.
                // Use CpmmPoolWithReserves::calculate_swap() directly when reserves are available.
                Err(crate::error::Error::protocol_error(
                    "RaydiumCpmm",
                    "calculate_swap requires vault reserves — use CpmmPoolWithReserves instead",
                ))
            }
            PoolState::RaydiumLaunchpad(pool) => {
                // Use bonding curve math with default 1% fee.
                // For precise fee rate, use LaunchpadSwapPool directly with config fee rate.
                raydium_launchpad::calculate_launchpad_swap(pool, 10_000, params)
            }
            PoolState::MeteoraDlmm(_pool) => {
                // DLMM reserves are in vault accounts, not inline.
                // Use DlmmSwapPool::calculate_swap() directly when reserves are available.
                Err(crate::error::Error::protocol_error(
                    "MeteoraDlmm",
                    "calculate_swap requires vault reserves — use DlmmSwapPool instead",
                ))
            }
            PoolState::MeteoraDbC(pool) => {
                // Bonding curve with inline reserves — use default 1% fee.
                // For precise fee rate, use DbcSwapPool directly with config fee rate.
                meteora_dbc::calculate_dbc_swap(pool, meteora_dbc::DEFAULT_FEE_RATE, params)
            }
        }
    }

    fn spot_price(&self) -> f64 {
        match self {
            PoolState::Pumpfun(curve) => SwapMath::spot_price(curve),
            PoolState::PumpSwap(pool) => SwapMath::spot_price(pool),
            PoolState::RaydiumV4(pool) => SwapMath::spot_price(pool),
            PoolState::RaydiumClmm(pool) => pool.current_price(),
            PoolState::RaydiumCpmm(pool) => pool.spot_price(),
            PoolState::RaydiumLaunchpad(pool) => pool.spot_price(),
            PoolState::MeteoraDlmm(pool) => pool.spot_price(),
            PoolState::MeteoraDbC(pool) => pool.spot_price(),
        }
    }

    fn is_active(&self) -> bool {
        match self {
            PoolState::Pumpfun(curve) => SwapMath::is_active(curve),
            PoolState::PumpSwap(pool) => SwapMath::is_active(pool),
            PoolState::RaydiumV4(pool) => SwapMath::is_active(pool),
            PoolState::RaydiumClmm(pool) => pool.is_active(),
            PoolState::RaydiumCpmm(pool) => pool.is_active(),
            PoolState::RaydiumLaunchpad(pool) => pool.is_active(),
            PoolState::MeteoraDlmm(pool) => pool.is_active(),
            PoolState::MeteoraDbC(pool) => pool.is_active(),
        }
    }
}

// =============================================================================
// Unified Pool Keys
// =============================================================================

/// Unified pool keys across all protocols.
///
/// This enum wraps protocol-specific pool keys/addresses, allowing unified
/// handling while preserving type safety for instruction building.
///
/// # Adding a New Protocol
///
/// 1. Add a variant wrapping the protocol's keys type
/// 2. Update all match arms (compiler will guide you)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PoolKeys {
    /// Pump.fun pool keys.
    Pumpfun(pumpfun::PumpfunKeys),

    /// PumpSwap pool keys (with PDA-derived creator vault + volume accumulators).
    PumpSwap(pumpswap::PumpSwapKeys),

    /// Raydium V4 pool keys.
    RaydiumV4(raydium_v4::RaydiumV4Keys),

    /// Raydium CLMM pool keys (concentrated liquidity).
    RaydiumClmm(raydium_clmm::RaydiumClmmKeys),

    /// Raydium CPMM pool keys (constant product with Token2022).
    RaydiumCpmm(raydium_cpmm::RaydiumCpmmKeys),

    /// Raydium Launchpad pool keys (bonding curve).
    RaydiumLaunchpad(raydium_launchpad::LaunchpadKeys),

    /// Meteora DLMM pool keys (bin-based concentrated liquidity).
    MeteoraDlmm(meteora_dlmm::DlmmKeys),

    /// Meteora DBC pool keys (dynamic bonding curve).
    MeteoraDbC(meteora_dbc::DbcKeys),
}

impl PoolKeys {
    /// Get the protocol for these pool keys.
    #[must_use]
    pub fn protocol(&self) -> Protocol {
        match self {
            PoolKeys::Pumpfun(_) => Protocol::Pumpfun,
            PoolKeys::PumpSwap(_) => Protocol::PumpSwap,
            PoolKeys::RaydiumV4(_) => Protocol::RaydiumV4,
            PoolKeys::RaydiumClmm(_) => Protocol::RaydiumClmm,
            PoolKeys::RaydiumCpmm(_) => Protocol::RaydiumCpmm,
            PoolKeys::RaydiumLaunchpad(_) => Protocol::RaydiumLaunchpad,
            PoolKeys::MeteoraDlmm(_) => Protocol::MeteoraDlmm,
            PoolKeys::MeteoraDbC(_) => Protocol::MeteoraDbC,
        }
    }

    /// Get the token mint address.
    #[must_use]
    pub fn mint(&self) -> &Pubkey {
        match self {
            PoolKeys::Pumpfun(keys) => &keys.mint,
            PoolKeys::PumpSwap(keys) => &keys.base_mint,
            PoolKeys::RaydiumV4(keys) => &keys.base_mint,
            PoolKeys::RaydiumClmm(keys) => &keys.token_mint_0,
            PoolKeys::RaydiumCpmm(keys) => &keys.token_0_mint,
            PoolKeys::RaydiumLaunchpad(keys) => &keys.base_mint,
            PoolKeys::MeteoraDlmm(keys) => &keys.token_x_mint,
            PoolKeys::MeteoraDbC(keys) => &keys.base_mint,
        }
    }

    /// Get the pool/curve address.
    #[must_use]
    pub fn pool_address(&self) -> &Pubkey {
        match self {
            PoolKeys::Pumpfun(keys) => &keys.bonding_curve,
            PoolKeys::PumpSwap(keys) => &keys.pool,
            PoolKeys::RaydiumV4(keys) => &keys.amm_id,
            PoolKeys::RaydiumClmm(keys) => &keys.pool_id,
            PoolKeys::RaydiumCpmm(keys) => &keys.pool_id,
            PoolKeys::RaydiumLaunchpad(keys) => &keys.pool_id,
            PoolKeys::MeteoraDlmm(keys) => &keys.lb_pair,
            PoolKeys::MeteoraDbC(keys) => &keys.pool,
        }
    }

    /// Unwrap as Pumpfun keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Pumpfun keys.
    #[must_use]
    pub fn as_pumpfun(&self) -> &pumpfun::PumpfunKeys {
        match self {
            PoolKeys::Pumpfun(keys) => keys,
            PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not Pumpfun keys")
            }
        }
    }

    /// Try to unwrap as Pumpfun keys.
    #[must_use]
    pub fn try_as_pumpfun(&self) -> Option<&pumpfun::PumpfunKeys> {
        match self {
            PoolKeys::Pumpfun(keys) => Some(keys),
            PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as PumpSwap keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not PumpSwap keys.
    #[must_use]
    pub fn as_pumpswap(&self) -> &pumpswap::PumpSwapKeys {
        match self {
            PoolKeys::PumpSwap(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not PumpSwap keys")
            }
        }
    }

    /// Try to unwrap as PumpSwap keys.
    #[must_use]
    pub fn try_as_pumpswap(&self) -> Option<&pumpswap::PumpSwapKeys> {
        match self {
            PoolKeys::PumpSwap(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Raydium V4 keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Raydium V4 keys.
    #[must_use]
    pub fn as_raydium_v4(&self) -> &raydium_v4::RaydiumV4Keys {
        match self {
            PoolKeys::RaydiumV4(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not Raydium V4 keys")
            }
        }
    }

    /// Try to unwrap as Raydium V4 keys.
    #[must_use]
    pub fn try_as_raydium_v4(&self) -> Option<&raydium_v4::RaydiumV4Keys> {
        match self {
            PoolKeys::RaydiumV4(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Raydium CLMM keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Raydium CLMM keys.
    #[must_use]
    pub fn as_raydium_clmm(&self) -> &raydium_clmm::RaydiumClmmKeys {
        match self {
            PoolKeys::RaydiumClmm(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not Raydium CLMM keys")
            }
        }
    }

    /// Try to unwrap as Raydium CLMM keys.
    #[must_use]
    pub fn try_as_raydium_clmm(&self) -> Option<&raydium_clmm::RaydiumClmmKeys> {
        match self {
            PoolKeys::RaydiumClmm(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Raydium CPMM keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Raydium CPMM keys.
    #[must_use]
    pub fn as_raydium_cpmm(&self) -> &raydium_cpmm::RaydiumCpmmKeys {
        match self {
            PoolKeys::RaydiumCpmm(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not Raydium CPMM keys")
            }
        }
    }

    /// Try to unwrap as Raydium CPMM keys.
    #[must_use]
    pub fn try_as_raydium_cpmm(&self) -> Option<&raydium_cpmm::RaydiumCpmmKeys> {
        match self {
            PoolKeys::RaydiumCpmm(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Raydium Launchpad keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Raydium Launchpad keys.
    #[must_use]
    pub fn as_raydium_launchpad(&self) -> &raydium_launchpad::LaunchpadKeys {
        match self {
            PoolKeys::RaydiumLaunchpad(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not Raydium Launchpad keys")
            }
        }
    }

    /// Try to unwrap as Raydium Launchpad keys.
    #[must_use]
    pub fn try_as_raydium_launchpad(&self) -> Option<&raydium_launchpad::LaunchpadKeys> {
        match self {
            PoolKeys::RaydiumLaunchpad(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::MeteoraDlmm(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Meteora DLMM keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Meteora DLMM keys.
    #[must_use]
    pub fn as_meteora_dlmm(&self) -> &meteora_dlmm::DlmmKeys {
        match self {
            PoolKeys::MeteoraDlmm(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDbC(_) => {
                panic!("not Meteora DLMM keys")
            }
        }
    }

    /// Try to unwrap as Meteora DLMM keys.
    #[must_use]
    pub fn try_as_meteora_dlmm(&self) -> Option<&meteora_dlmm::DlmmKeys> {
        match self {
            PoolKeys::MeteoraDlmm(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDbC(_) => None,
        }
    }

    /// Unwrap as Meteora DBC keys.
    ///
    /// # Panics
    ///
    /// Panics if this is not Meteora DBC keys.
    #[must_use]
    pub fn as_meteora_dbc(&self) -> &meteora_dbc::DbcKeys {
        match self {
            PoolKeys::MeteoraDbC(keys) => keys,
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_) => {
                panic!("not Meteora DBC keys")
            }
        }
    }

    /// Try to unwrap as Meteora DBC keys.
    #[must_use]
    pub fn try_as_meteora_dbc(&self) -> Option<&meteora_dbc::DbcKeys> {
        match self {
            PoolKeys::MeteoraDbC(keys) => Some(keys),
            PoolKeys::Pumpfun(_)
            | PoolKeys::PumpSwap(_)
            | PoolKeys::RaydiumV4(_)
            | PoolKeys::RaydiumClmm(_)
            | PoolKeys::RaydiumCpmm(_)
            | PoolKeys::RaydiumLaunchpad(_)
            | PoolKeys::MeteoraDlmm(_) => None,
        }
    }
}

impl From<pumpfun::PumpfunKeys> for PoolKeys {
    fn from(keys: pumpfun::PumpfunKeys) -> Self {
        PoolKeys::Pumpfun(keys)
    }
}

impl From<raydium_v4::RaydiumV4Keys> for PoolKeys {
    fn from(keys: raydium_v4::RaydiumV4Keys) -> Self {
        PoolKeys::RaydiumV4(keys)
    }
}

impl From<pumpswap::PumpSwapKeys> for PoolKeys {
    fn from(keys: pumpswap::PumpSwapKeys) -> Self {
        PoolKeys::PumpSwap(keys)
    }
}

impl From<raydium_clmm::RaydiumClmmKeys> for PoolKeys {
    fn from(keys: raydium_clmm::RaydiumClmmKeys) -> Self {
        PoolKeys::RaydiumClmm(keys)
    }
}

impl From<raydium_cpmm::RaydiumCpmmKeys> for PoolKeys {
    fn from(keys: raydium_cpmm::RaydiumCpmmKeys) -> Self {
        PoolKeys::RaydiumCpmm(keys)
    }
}

impl From<raydium_launchpad::LaunchpadKeys> for PoolKeys {
    fn from(keys: raydium_launchpad::LaunchpadKeys) -> Self {
        PoolKeys::RaydiumLaunchpad(keys)
    }
}

impl From<meteora_dlmm::DlmmKeys> for PoolKeys {
    fn from(keys: meteora_dlmm::DlmmKeys) -> Self {
        PoolKeys::MeteoraDlmm(keys)
    }
}

impl From<meteora_dbc::DbcKeys> for PoolKeys {
    fn from(keys: meteora_dbc::DbcKeys) -> Self {
        PoolKeys::MeteoraDbC(keys)
    }
}

impl Protocol {
    /// Every protocol variant. The single list the reverse lookup walks;
    /// completeness is enforced by `all_contains_every_variant`, which cannot
    /// compile if a variant is added without being listed here.
    pub const ALL: [Protocol; 9] = [
        Protocol::Pumpfun,
        Protocol::PumpSwap,
        Protocol::RaydiumV4,
        Protocol::RaydiumClmm,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumLaunchpad,
        Protocol::MeteoraDlmm,
        Protocol::MeteoraDbC,
        Protocol::MeteoraDammV2,
    ];

    /// Get the on-chain program ID for this protocol.
    #[must_use]
    pub fn program_id(&self) -> Pubkey {
        match self {
            Protocol::Pumpfun => pumpfun::PROGRAM_ID,
            Protocol::PumpSwap => pumpswap::PROGRAM_ID,
            Protocol::RaydiumV4 => raydium_v4::PROGRAM_ID,
            Protocol::RaydiumClmm => raydium_clmm::PROGRAM_ID,
            Protocol::RaydiumCpmm => raydium_cpmm::PROGRAM_ID,
            Protocol::RaydiumLaunchpad => raydium_launchpad::PROGRAM_ID,
            Protocol::MeteoraDlmm => meteora_dlmm::PROGRAM_ID,
            Protocol::MeteoraDbC => meteora_dbc::PROGRAM_ID,
            Protocol::MeteoraDammV2 => meteora_damm_v2::PROGRAM_ID,
        }
    }

    /// Get human-readable name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Protocol::Pumpfun => "Pump.fun",
            Protocol::PumpSwap => "PumpSwap",
            Protocol::RaydiumV4 => "Raydium V4",
            Protocol::RaydiumClmm => "Raydium CLMM",
            Protocol::RaydiumCpmm => "Raydium CPMM",
            Protocol::RaydiumLaunchpad => "Raydium Launchpad",
            Protocol::MeteoraDlmm => "Meteora DLMM",
            Protocol::MeteoraDbC => "Meteora DBC",
            Protocol::MeteoraDammV2 => "Meteora DAMM v2",
        }
    }

    /// Get short identifier (for logging, metrics).
    #[must_use]
    pub fn short_name(&self) -> &'static str {
        match self {
            Protocol::Pumpfun => "pumpfun",
            Protocol::PumpSwap => "pumpswap",
            Protocol::RaydiumV4 => "raydium_v4",
            Protocol::RaydiumClmm => "raydium_clmm",
            Protocol::RaydiumCpmm => "raydium_cpmm",
            Protocol::RaydiumLaunchpad => "raydium_launchpad",
            Protocol::MeteoraDlmm => "meteora_dlmm",
            Protocol::MeteoraDbC => "meteora_dbc",
            Protocol::MeteoraDammV2 => "meteora_damm_v2",
        }
    }

    /// Check if protocol uses bonding curves (vs AMM pools).
    #[must_use]
    pub fn is_bonding_curve(&self) -> bool {
        match self {
            Protocol::Pumpfun => true,
            Protocol::RaydiumLaunchpad => true,
            Protocol::MeteoraDbC => true,
            Protocol::PumpSwap
            | Protocol::RaydiumV4
            | Protocol::RaydiumClmm
            | Protocol::RaydiumCpmm
            | Protocol::MeteoraDlmm
            | Protocol::MeteoraDammV2 => false,
        }
    }

    /// Check if protocol supports concentrated liquidity.
    #[must_use]
    pub fn is_concentrated_liquidity(&self) -> bool {
        match self {
            Protocol::RaydiumClmm => true,
            Protocol::MeteoraDlmm => true,
            Protocol::MeteoraDammV2 => true,
            Protocol::Pumpfun
            | Protocol::PumpSwap
            | Protocol::RaydiumV4
            | Protocol::RaydiumCpmm
            | Protocol::RaydiumLaunchpad
            | Protocol::MeteoraDbC => false,
        }
    }

    /// Get the base token for this protocol (usually SOL or USDC).
    #[must_use]
    pub fn base_token(&self) -> crate::tokens::BaseToken {
        match self {
            Protocol::Pumpfun
            | Protocol::PumpSwap
            | Protocol::RaydiumV4
            | Protocol::RaydiumClmm
            | Protocol::RaydiumCpmm
            | Protocol::RaydiumLaunchpad
            | Protocol::MeteoraDammV2
            | Protocol::MeteoraDlmm
            | Protocol::MeteoraDbC => crate::tokens::BaseToken::Sol,
        }
    }

    /// Get all supported protocols.
    #[must_use]
    pub fn all() -> &'static [Protocol] {
        &[
            Protocol::Pumpfun,
            Protocol::PumpSwap,
            Protocol::RaydiumV4,
            Protocol::RaydiumClmm,
            Protocol::RaydiumCpmm,
            Protocol::RaydiumLaunchpad,
            Protocol::MeteoraDlmm,
            Protocol::MeteoraDbC,
        ]
    }

    /// Reverse of [`program_id`](Self::program_id).
    ///
    /// Derived FROM `program_id`'s exhaustive match rather than restating it:
    /// the previous hand-written if-chain silently omitted `MeteoraDammV2`,
    /// so every direct DAMM v2 call was misread as an unknown router (4,096
    /// swaps in one live hour, 2026-08-11). A second hand-maintained list of
    /// a mechanical fact is the bug; there is now only one.
    #[must_use]
    pub fn from_program_id(program_id: &Pubkey) -> Option<Protocol> {
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.program_id() == *program_id)
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::state::Legacy;
    use crate::traits::SwapParams;

    fn make_test_curve() -> pumpfun::BondingCurve {
        pumpfun::BondingCurve {
            virtual_token_reserves: 1_000_000_000_000_000,
            virtual_sol_reserves: 30_000_000_000,
            real_token_reserves: 800_000_000_000_000,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
            creator: Pubkey::new_unique(),
            is_mayhem_mode: Legacy::Present(false),
            is_cashback_coin: Legacy::Present(false),
        }
    }

    fn make_account_data(curve: &pumpfun::BondingCurve) -> Vec<u8> {
        let mut data = Vec::with_capacity(pumpfun::BONDING_CURVE_ACCOUNT_SIZE);
        data.extend_from_slice(&pumpfun::BONDING_CURVE_DISCRIMINATOR);
        data.extend_from_slice(&curve.virtual_token_reserves.to_le_bytes());
        data.extend_from_slice(&curve.virtual_sol_reserves.to_le_bytes());
        data.extend_from_slice(&curve.real_token_reserves.to_le_bytes());
        data.extend_from_slice(&curve.real_sol_reserves.to_le_bytes());
        data.extend_from_slice(&curve.token_total_supply.to_le_bytes());
        data.push(if curve.complete { 1 } else { 0 });
        data.extend_from_slice(curve.creator.as_ref());
        data
    }

    // =========================================================================
    // Protocol Tests
    // =========================================================================

    #[test]
    fn protocol_program_id() {
        let pumpfun = Protocol::Pumpfun;
        assert_eq!(pumpfun.program_id(), pumpfun::PROGRAM_ID);
    }

    #[test]
    fn protocol_from_program_id() {
        let found = Protocol::from_program_id(&pumpfun::PROGRAM_ID);
        assert_eq!(found, Some(Protocol::Pumpfun));

        let not_found = Protocol::from_program_id(&Pubkey::new_unique());
        assert_eq!(not_found, None);
    }

    #[test]
    fn all_protocols_have_unique_ids() {
        let protocols = Protocol::all();
        let mut ids: Vec<_> = protocols.iter().map(|p| p.program_id()).collect();
        let original_len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "Duplicate program IDs found");
    }

    #[test]
    fn protocol_display() {
        assert_eq!(Protocol::Pumpfun.to_string(), "Pump.fun");
    }

    // =========================================================================
    // PoolState Tests
    // =========================================================================

    #[test]
    fn pool_state_from_account_data() {
        let curve = make_test_curve();
        let data = make_account_data(&curve);

        let state = PoolState::from_account_data(Protocol::Pumpfun, &data).unwrap();

        assert_eq!(state.protocol(), Protocol::Pumpfun);
        assert!(state.is_active());
    }

    #[test]
    fn pool_state_detect_and_parse() {
        let curve = make_test_curve();
        let data = make_account_data(&curve);

        let result = PoolState::detect_and_parse(&data);
        assert!(result.is_some());

        let (protocol, state) = result.unwrap();
        assert_eq!(protocol, Protocol::Pumpfun);
        assert_eq!(state.protocol(), Protocol::Pumpfun);
    }

    #[test]
    fn pool_state_detect_and_parse_unknown() {
        // Random data that doesn't match any protocol
        let data = vec![0xFF; 100];
        let result = PoolState::detect_and_parse(&data);
        assert!(result.is_none());
    }

    #[test]
    fn pool_state_swap_math() {
        let curve = make_test_curve();
        let data = make_account_data(&curve);
        let state = PoolState::from_account_data(Protocol::Pumpfun, &data).unwrap();

        // Test via SwapMath trait
        let params = SwapParams::buy(1_000_000_000); // 1 SOL
        let output = state.calculate_swap(&params).unwrap();

        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
    }

    #[test]
    fn pool_state_as_pumpfun() {
        let curve = make_test_curve();
        let data = make_account_data(&curve);
        let state = PoolState::from_account_data(Protocol::Pumpfun, &data).unwrap();

        // as_pumpfun should work
        let unwrapped = state.as_pumpfun();
        assert_eq!(
            unwrapped.virtual_token_reserves,
            curve.virtual_token_reserves
        );

        // try_as_pumpfun should also work
        let try_unwrapped = state.try_as_pumpfun();
        assert!(try_unwrapped.is_some());
    }

    #[test]
    fn pool_state_spot_price() {
        let curve = make_test_curve();
        let data = make_account_data(&curve);
        let state = PoolState::from_account_data(Protocol::Pumpfun, &data).unwrap();

        let price = state.spot_price();
        assert!(price > 0.0);
        assert!((price - curve.spot_price()).abs() < 0.0000001);
    }

    // =========================================================================
    // PoolKeys Tests
    // =========================================================================

    #[test]
    fn pool_keys_from_pumpfun() {
        let mint = Pubkey::new_unique();
        let creator = Pubkey::new_unique();

        let pumpfun_keys = pumpfun::PumpfunKeys::new(mint, creator);
        let pool_keys: PoolKeys = pumpfun_keys.clone().into();

        assert_eq!(pool_keys.protocol(), Protocol::Pumpfun);
        assert_eq!(pool_keys.mint(), &mint);
        assert_eq!(pool_keys.pool_address(), &pumpfun_keys.bonding_curve);
    }

    #[test]
    fn pool_keys_as_pumpfun() {
        let mint = Pubkey::new_unique();
        let creator = Pubkey::new_unique();

        let pumpfun_keys = pumpfun::PumpfunKeys::new(mint, creator);
        let pool_keys: PoolKeys = pumpfun_keys.clone().into();

        // as_pumpfun should work
        let unwrapped = pool_keys.as_pumpfun();
        assert_eq!(unwrapped.mint, mint);

        // try_as_pumpfun should also work
        let try_unwrapped = pool_keys.try_as_pumpfun();
        assert!(try_unwrapped.is_some());
        assert_eq!(try_unwrapped.unwrap().mint, mint);
    }
}

#[cfg(test)]
mod protocol_all_tests {
    use super::*;

    /// `ALL` must list every variant, and `from_program_id` must invert
    /// `program_id` for each. The old hand-written if-chain omitted
    /// `MeteoraDammV2`, so direct DAMM v2 swaps were classified as unknown
    /// routers — this is the test that makes that unrepresentable.
    #[test]
    fn all_contains_every_variant() {
        // Exhaustive: adding a Protocol variant fails to compile here until
        // it is given a slot, and the assert then fails until it is in ALL.
        fn slot(p: Protocol) -> usize {
            match p {
                Protocol::Pumpfun => 0,
                Protocol::PumpSwap => 1,
                Protocol::RaydiumV4 => 2,
                Protocol::RaydiumClmm => 3,
                Protocol::RaydiumCpmm => 4,
                Protocol::RaydiumLaunchpad => 5,
                Protocol::MeteoraDlmm => 6,
                Protocol::MeteoraDbC => 7,
                Protocol::MeteoraDammV2 => 8,
            }
        }
        let mut seen = [false; 9];
        for p in Protocol::ALL {
            seen[slot(p)] = true;
        }
        assert!(seen.iter().all(|s| *s), "every variant must appear in ALL");
    }

    #[test]
    fn from_program_id_inverts_program_id_for_every_protocol() {
        for p in Protocol::ALL {
            assert_eq!(
                Protocol::from_program_id(&p.program_id()),
                Some(p),
                "{p:?} must resolve from its own program id"
            );
        }
        assert_eq!(Protocol::from_program_id(&Pubkey::new_unique()), None);
    }
}
