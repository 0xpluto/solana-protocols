//! `solana-account-traits` handlers for Meteora DLMM accounts.
//!
//! Four handlers register under the same program id, each gated by
//! its own 8-byte Anchor discriminator. The ingest task
//! dispatches inbound account updates to the matching handler purely
//! by `(program_id, discriminator)` — we never have to peek at struct
//! shape.
//!
//! Only compiled when the `cache-handlers` feature is enabled.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::OnchainAccount;
use solana_account_traits::{CacheInsert, HandleResult, HandlerError, StorageHandler};

use super::constants::{
    BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR, BIN_ARRAY_DISCRIMINATOR, LB_PAIR_DISCRIMINATOR,
    POSITION_V2_DISCRIMINATOR, PROGRAM_ID,
};
use super::state::{
    decode_bin_array, decode_bin_array_bitmap_extension, decode_lb_pair, decode_position_v2_full,
    BinArray, BinArrayBitmapExtension, LbPair, LbPairExt, PositionV2Full,
};

// =====================================================================
// LbPair (the pool itself)
// =====================================================================

/// Handler for `LbPair` account updates.
///
/// Surfaces the active-bin spot price into [`HandleResult`] so the
/// cache snapshot can serialize a price row alongside the raw account
/// — same convention as Pump.fun's bonding-curve handler.
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PROGRAM_ID,
    state = LbPair,
    discriminator_const = LB_PAIR_DISCRIMINATOR,
    decode = decode_lb_pair,
    fixture = "meteora_dlmm/lb_pair.json"
)]
pub struct LbPairHandler;

impl LbPairHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for LbPairHandler
where
    C: CacheInsert<Pubkey, LbPair> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        let price = state.spot_price();
        cache.insert(*pubkey, state.clone(), slot);
        Ok(HandleResult {
            spot_price: Some(price),
            ..Default::default()
        })
    }
}

// =====================================================================
// BinArray
// =====================================================================

/// Handler for `BinArray` account updates. ~70 bins of liquidity per
/// array; the trader needs these to walk the curve when quoting.
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PROGRAM_ID,
    state = BinArray,
    discriminator_const = BIN_ARRAY_DISCRIMINATOR,
    decode = decode_bin_array,
    fixture = "meteora_dlmm/bin_array.json"
)]
pub struct BinArrayHandler;

impl BinArrayHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for BinArrayHandler
where
    C: CacheInsert<Pubkey, BinArray> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        cache.insert(*pubkey, state.clone(), slot);
        Ok(HandleResult::default())
    }
}

// =====================================================================
// BinArrayBitmapExtension
// =====================================================================

/// Handler for `BinArrayBitmapExtension` updates. Optional account —
/// only present on pools whose active bin range exceeds the inline
/// 16×u64 bitmap stored on the [`LbPair`] itself.
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PROGRAM_ID,
    state = BinArrayBitmapExtension,
    discriminator_const = BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR,
    decode = decode_bin_array_bitmap_extension,
    fixture = "meteora_dlmm/bin_array_bitmap_extension.json"
)]
pub struct BinArrayBitmapExtensionHandler;

impl BinArrayBitmapExtensionHandler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for BinArrayBitmapExtensionHandler
where
    C: CacheInsert<Pubkey, BinArrayBitmapExtension> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        cache.insert(*pubkey, state.clone(), slot);
        Ok(HandleResult::default())
    }
}

// =====================================================================
// PositionV2
// =====================================================================

/// Handler for `PositionV2` account updates. Tracks every LP position
/// owned by the wallets covered by the ingest filter — the MM bot
/// will be subscribed to its own positions; analytics consumers may
/// be subscribed to all of them.
// Full decode: fixed header + any appended bins (dynamic positions wider than
// 70 bins). The cache stores the complete view so downstream metrics see every
// bin.
#[derive(Debug, Default, Clone, Copy, OnchainAccount)]
#[onchain(
    program = PROGRAM_ID,
    state = PositionV2Full,
    discriminator_const = POSITION_V2_DISCRIMINATOR,
    decode = decode_position_v2_full,
    fixture = "meteora_dlmm/position_v2_narrow.json"
)]
pub struct PositionV2Handler;

impl PositionV2Handler {
    pub const fn new() -> Self {
        Self
    }
}

impl<C> StorageHandler<C> for PositionV2Handler
where
    C: CacheInsert<Pubkey, PositionV2Full> + Send + Sync + 'static,
{
    fn apply(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        state: &Self::State,
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        cache.insert(*pubkey, state.clone(), slot);
        Ok(HandleResult::default())
    }
}
