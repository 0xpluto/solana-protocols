//! `solana-account-traits` handlers for Pump.fun accounts.
//!
//! Three handlers register under two program IDs, all via the 8-byte
//! Anchor discriminator path:
//!
//! * [`PumpfunBondingCurveHandler`] — bonding curve accounts owned by
//!   [`PROGRAM_ID`]. The 81 / 83 / 150 / 151-byte layouts coexist on-chain
//!   (successive program upgrades), but they all carry the same
//!   `account:BondingCurve` discriminator — one handler covers every
//!   size.
//! * [`PumpfunGlobalHandler`] — the single Global PDA owned by
//!   [`PROGRAM_ID`]. Keeps `PumpfunFeeRecipients` live in the cache.
//! * [`PumpfunFeeConfigHandler`] — the single FeeConfig PDA owned by
//!   **[`PUMP_FEES_PROGRAM_ID`]** (not `PROGRAM_ID` — different program).
//!   Keeps `PumpfunFeeConfig` live in the cache.
//!
//! Only compiled when the `cache-handlers` feature is enabled.

use solana_account_traits::{
    CacheInsert, CacheSingleton, HandleResult, HandlerError, StorageHandler,
};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::OnchainAccount;

use super::super::pumpswap::{PumpSwapFeeConfig, FEE_CONFIG as PUMPSWAP_FEE_CONFIG};
use super::constants::{
    BONDING_CURVE_DISCRIMINATOR, FEE_CONFIG_DISCRIMINATOR, FEE_CONFIG_PDA, GLOBAL_DISCRIMINATOR,
    PROGRAM_ID, PUMP_FEES_PROGRAM_ID,
};
use super::fee_config::PumpfunFeeConfig;
use super::global::PumpfunFeeRecipients;
use super::state::BondingCurve;

// =====================================================================
// Bonding curve
// =====================================================================

/// Handler for Pump.fun bonding curve account updates.
///
/// Dispatched via the 8-byte Anchor discriminator for
/// `account:BondingCurve`. The 81 / 83 / 150 / 151-byte on-chain layouts
/// all share this discriminator, so a single handler covers every
/// variant — [`BondingCurve::from_account_data`] reads the core prefix
/// and treats trailing bytes as optional.
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PROGRAM_ID,
    state = BondingCurve,
    discriminator_const = BONDING_CURVE_DISCRIMINATOR,
    decode = BondingCurve::from_account_data,
    fixture = "pumpfun/bonding_curve_150.json"
)]
pub struct PumpfunBondingCurveHandler;

impl PumpfunBondingCurveHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for PumpfunBondingCurveHandler
where
    C: CacheInsert<Pubkey, BondingCurve> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        cache.insert(*pubkey, *state, slot);
        Ok(HandleResult {
            spot_price: Some(state.spot_price()),
            ..Default::default()
        })
    }
}

// =====================================================================
// Global PDA — fee recipients
// =====================================================================

/// Handler for the Pumpfun Global PDA (`4wTV1...`). Extracts the two
/// active fee recipients and writes them into the cache as a singleton.
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PROGRAM_ID,
    state = PumpfunFeeRecipients,
    discriminator_const = GLOBAL_DISCRIMINATOR,
    decode = PumpfunFeeRecipients::from_account_data,
    fixture = "pumpfun/global.json"
)]
pub struct PumpfunGlobalHandler;

impl PumpfunGlobalHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for PumpfunGlobalHandler
where
    C: CacheSingleton<PumpfunFeeRecipients> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        _pubkey: &Pubkey,
        state: &Self::State,
        _slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        cache.set(*state);
        Ok(HandleResult::default())
    }
}

// =====================================================================
// FeeConfig PDA — dynamic fee tiers
// =====================================================================

/// Handler for the `pump_fees` program's **FeeConfig family** — both pumpfun's
/// PDA (`8Wf5Ti…`) and PumpSwap's (`5PHirr…`).
///
/// It necessarily owns both: they share an owner *and* an `account:FeeConfig`
/// discriminator (verified on mainnet — identical 4073-byte layout), which is
/// the registry's entire dispatch key, so one handler matches both and a second
/// handler could not even be registered. The only place the two are
/// distinguishable is [`apply`](StorageHandler::apply), which routes on the
/// account's **pubkey** into separate cache slots. Before that routing existed,
/// a PumpSwap config update silently overwrote pumpfun's singleton and pumpfun
/// quoting read PumpSwap's fee tiers.
///
/// A FeeConfig PDA for any *third* consuming program decodes fine but has no
/// slot: it is skipped with a warning, never written over one of the two we
/// model.
///
/// [`calculate_fee_tier`]: super::fee_config::calculate_fee_tier
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PUMP_FEES_PROGRAM_ID,
    state = PumpfunFeeConfig,
    discriminator_const = FEE_CONFIG_DISCRIMINATOR,
    decode = PumpfunFeeConfig::from_account_data,
    fixture = "pump_fees/fee_config.json"
)]
pub struct PumpfunFeeConfigHandler;

impl PumpfunFeeConfigHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for PumpfunFeeConfigHandler
where
    C: CacheSingleton<PumpfunFeeConfig> + CacheSingleton<PumpSwapFeeConfig> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        _slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        // Route by PDA: the discriminator cannot tell these apart (see the type
        // docs), so the pubkey is the only disambiguator.
        if *pubkey == FEE_CONFIG_PDA {
            <C as CacheSingleton<PumpfunFeeConfig>>::set(cache, state.clone());
        } else if *pubkey == PUMPSWAP_FEE_CONFIG {
            <C as CacheSingleton<PumpSwapFeeConfig>>::set(cache, PumpSwapFeeConfig(state.clone()));
        } else {
            // A FeeConfig for some other consuming program. Decodes fine, but we
            // model no slot for it — skip loudly rather than overwrite one of
            // ours (a new consuming program is worth noticing, not absorbing).
            tracing::warn!(%pubkey, "pump_fees FeeConfig for an unmodelled program; skipped");
        }
        Ok(HandleResult::default())
    }
}
