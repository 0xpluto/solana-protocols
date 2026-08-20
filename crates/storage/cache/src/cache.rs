//! `LocalCache` — the composed, clone-able, slot-versioned cache.
//!
//! Field storage lives in [`CacheData`]. `LocalCache` wraps it in an `Arc`
//! alongside shared [`CacheMeta`]; cloning a `LocalCache` is two refcount
//! bumps. The same `CacheData` type is serialized to disk on save — no
//! mirror struct to keep in sync.
//!
//! ## Adding a field
//!
//! 1. Add it to [`CacheData`] (type `VersionedStateMap<K, V>` — derives
//!    `Serialize`/`Deserialize`, so persistence Just Works).
//! 2. Add a `usize` entry to [`FieldDepths`] and initialize it in
//!    [`CacheData::with_depths`].
//! 3. Add one `impl_cache_access!(LocalCache, data.<field>, K, V);` line so
//!    callers can bound on `CacheGet<K, V>` / `CacheInsert<K, V>`.
//!
//! Snapshot / SnapshotRef duplication is gone — `CacheData` plays both roles.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use solana_account_traits::{impl_cache_access, CacheSingleton};
use solana_protocols::meteora_dlmm::{BinArray, BinArrayBitmapExtension, LbPair, PositionV2Full};
use solana_protocols::pumpfun::{BondingCurve, PumpfunFeeConfig, PumpfunFeeRecipients};
use solana_protocols::pumpswap::{PumpSwapFeeConfig, PumpSwapPool};
use solana_protocols::tokens::TokenAccount;
use solana_protocols::TokenProgram;
use solana_pubkey::Pubkey;

use crate::persist::{self, CacheError};
use crate::versioned_map::VersionedStateMap;

/// Per-field history depths.
#[derive(Debug, Clone)]
pub struct FieldDepths {
    pub blockhashes: usize,
    pub bonding_curves: usize,
    /// History depth for PumpSwap AMM pool accounts.
    pub pumpswap_pools: usize,
    /// History depth for Meteora DLMM `LbPair` pool accounts.
    pub dlmm_lb_pairs: usize,
    /// History depth for Meteora DLMM `BinArray` accounts.
    /// Bin arrays update on every swap that crosses them, so a small
    /// depth (≥3) keeps the recent state for routing without ballooning
    /// memory; ~70 bins × ~3 versions per array × N watched pools.
    pub dlmm_bin_arrays: usize,
    /// History depth for `BinArrayBitmapExtension` accounts (sparse pools).
    pub dlmm_bin_array_bitmap_extensions: usize,
    /// History depth for `PositionV2` accounts (LP positions).
    pub dlmm_positions: usize,
    /// History depth for the mint → token-program map. A mint's owning
    /// program never changes, so 1 is always correct.
    pub mint_token_programs: usize,
    /// History depth for token accounts (AMM vault balances). These are the
    /// reserve feed, so a few versions back are useful for the same
    /// slot-lookback reasons as the pools they belong to.
    pub token_accounts: usize,
}

impl Default for FieldDepths {
    fn default() -> Self {
        Self {
            blockhashes: 8,
            bonding_curves: 8,
            pumpswap_pools: 8,
            dlmm_lb_pairs: 8,
            dlmm_bin_arrays: 4,
            dlmm_bin_array_bitmap_extensions: 4,
            dlmm_positions: 8,
            mint_token_programs: 1,
            token_accounts: 8,
        }
    }
}

/// Construction-time configuration.
#[derive(Debug, Clone, Default)]
pub struct LocalCacheConfig {
    pub depths: FieldDepths,
    /// If set, the final [`LocalCache`] clone to drop writes a snapshot here.
    /// Callers that want guaranteed persistence should also call
    /// [`LocalCache::save`] explicitly during shutdown — the drop path is a
    /// safety net, not a replacement.
    pub persist_on_drop: Option<PathBuf>,
    /// Optional shared atomic for the cache's observed on-chain slot.
    /// `None` → cache creates its own (starts at 0). `Some(arc)` → cache
    /// uses the provided atomic, which external writers (the gRPC
    /// subscriber, a replay harness, …) hold a clone of and update. Any
    /// cache consumer can then read the slot via [`LocalCache::current_slot`]
    /// without needing a handle to the writer.
    pub current_slot: Option<Arc<AtomicU64>>,
}

/// The single source of truth for cache field layout.
///
/// Derives `Serialize`/`Deserialize` so this one type serves both in-memory
/// storage and on-disk persistence. Runtime-only fields — live on-chain
/// state that's repopulated from gRPC on startup — are marked
/// `#[serde(skip, default = "…")]` so they stay out of the on-disk snapshot
/// and are replaced with empty placeholders on load. [`LocalCache::load_from`]
/// then calls [`reset_runtime_fields`](Self::reset_runtime_fields) to align
/// their depths with the loaded config.
#[derive(Debug, Serialize, Deserialize)]
struct CacheData {
    blockhashes: VersionedStateMap<Pubkey, [u8; 32]>,

    #[serde(skip, default = "default_bonding_curves_map")]
    bonding_curves: VersionedStateMap<Pubkey, BondingCurve>,

    /// PumpSwap AMM pools — populated from the PumpSwap program's
    /// account stream for tokens that have graduated from pumpfun.
    /// Runtime-only (not snapshotted to disk); repopulated from
    /// gRPC on startup.
    #[serde(skip, default = "default_pumpswap_pools_map")]
    pumpswap_pools: VersionedStateMap<Pubkey, PumpSwapPool>,

    /// Meteora DLMM `LbPair` pool accounts. Updated on every swap +
    /// liquidity event on a watched pool. Runtime-only.
    #[serde(skip, default = "default_dlmm_lb_pairs_map")]
    dlmm_lb_pairs: VersionedStateMap<Pubkey, LbPair>,

    /// Meteora DLMM `BinArray` accounts. One pool has many; the
    /// active-bin walk (quote / route) loads 1–3 per direction.
    /// Runtime-only.
    #[serde(skip, default = "default_dlmm_bin_arrays_map")]
    dlmm_bin_arrays: VersionedStateMap<Pubkey, BinArray>,

    /// `BinArrayBitmapExtension` accounts — optional, only present
    /// for pools whose active bin range exceeds the inline 16×u64
    /// bitmap on the [`LbPair`] itself. Runtime-only.
    #[serde(skip, default = "default_dlmm_bin_array_bitmap_extensions_map")]
    dlmm_bin_array_bitmap_extensions: VersionedStateMap<Pubkey, BinArrayBitmapExtension>,

    /// `PositionV2` accounts owned by wallets the ingest filter
    /// covers — typically the bot's own positions when MM is live.
    /// Runtime-only.
    #[serde(skip, default = "default_dlmm_positions_map")]
    dlmm_positions: VersionedStateMap<Pubkey, PositionV2Full>,

    /// Mint → owning token program (classic SPL vs Token-2022) — the fact
    /// that decides ATA derivation and the `token_program` account of every
    /// swap we build. Fed passively (creations/swaps observed on the
    /// firehose name it; an RPC `get_account(mint).owner` fallback covers
    /// cold misses). Unlike the fields above this is an *immutable* fact,
    /// not live state — still runtime-only for now; the resolver's miss
    /// metric decides whether persisting it ever earns its snapshot bytes.
    #[serde(skip, default = "default_mint_token_programs_map")]
    mint_token_programs: VersionedStateMap<Pubkey, TokenProgram>,

    /// Token accounts — chiefly **AMM vault balances**, which is where pools
    /// like PumpSwap actually keep their reserves (the pool account holds only
    /// the vault addresses). Populated by `TokenAccountHandler` from accounts
    /// the pool handlers report as dependencies. Runtime-only.
    #[serde(skip, default = "default_token_accounts_map")]
    token_accounts: VersionedStateMap<Pubkey, TokenAccount>,

    /// Pumpfun's active fee recipients — `fee_recipients[0]` from the
    /// Global PDA (regular) + `reserved_fee_recipients[0]` (mayhem). Needed
    /// to build correct buy/sell instructions for post-cashback curves.
    /// Runtime-only: repopulated from the Global PDA gRPC stream on startup.
    #[serde(skip, default = "default_singleton_lock")]
    pumpfun_fee_recipients: RwLock<Option<PumpfunFeeRecipients>>,

    /// Pumpfun's current fee tiers — `flat_fees` + market-cap-keyed
    /// `fee_tiers`. Needed for accurate quoting post-cashback; the old
    /// hardcoded 95 bps / 30 bps constants only match the smallest tier.
    /// Runtime-only: repopulated from the FeeConfig PDA gRPC stream.
    #[serde(skip, default = "default_singleton_lock")]
    pumpfun_fee_config: RwLock<Option<PumpfunFeeConfig>>,

    /// PumpSwap's fee tiers. A **separate slot** from `pumpfun_fee_config`
    /// despite the identical layout: the two PDAs share an owner and
    /// discriminator, so one handler receives both and routes by pubkey — with
    /// one slot, whichever updated last would overwrite the other and pumpfun
    /// quoting would read PumpSwap's tiers. Runtime-only.
    #[serde(skip, default = "default_singleton_lock")]
    pumpswap_fee_config: RwLock<Option<PumpSwapFeeConfig>>,
}

fn default_bonding_curves_map() -> VersionedStateMap<Pubkey, BondingCurve> {
    // Depth 0 placeholder — real depth is set by `reset_runtime_fields`
    // after load from the caller's configured `FieldDepths`.
    VersionedStateMap::with_depth(0)
}

fn default_pumpswap_pools_map() -> VersionedStateMap<Pubkey, PumpSwapPool> {
    VersionedStateMap::with_depth(0)
}

fn default_dlmm_lb_pairs_map() -> VersionedStateMap<Pubkey, LbPair> {
    VersionedStateMap::with_depth(0)
}

fn default_dlmm_bin_arrays_map() -> VersionedStateMap<Pubkey, BinArray> {
    VersionedStateMap::with_depth(0)
}

fn default_dlmm_bin_array_bitmap_extensions_map(
) -> VersionedStateMap<Pubkey, BinArrayBitmapExtension> {
    VersionedStateMap::with_depth(0)
}

fn default_dlmm_positions_map() -> VersionedStateMap<Pubkey, PositionV2Full> {
    VersionedStateMap::with_depth(0)
}

fn default_mint_token_programs_map() -> VersionedStateMap<Pubkey, TokenProgram> {
    VersionedStateMap::with_depth(0)
}

fn default_token_accounts_map() -> VersionedStateMap<Pubkey, TokenAccount> {
    VersionedStateMap::with_depth(0)
}

fn default_singleton_lock<T>() -> RwLock<Option<T>> {
    RwLock::new(None)
}

impl CacheData {
    fn with_depths(depths: &FieldDepths) -> Self {
        Self {
            blockhashes: VersionedStateMap::with_depth(depths.blockhashes),
            bonding_curves: VersionedStateMap::with_depth(depths.bonding_curves),
            pumpswap_pools: VersionedStateMap::with_depth(depths.pumpswap_pools),
            dlmm_lb_pairs: VersionedStateMap::with_depth(depths.dlmm_lb_pairs),
            dlmm_bin_arrays: VersionedStateMap::with_depth(depths.dlmm_bin_arrays),
            dlmm_bin_array_bitmap_extensions: VersionedStateMap::with_depth(
                depths.dlmm_bin_array_bitmap_extensions,
            ),
            dlmm_positions: VersionedStateMap::with_depth(depths.dlmm_positions),
            mint_token_programs: VersionedStateMap::with_depth(depths.mint_token_programs),
            token_accounts: VersionedStateMap::with_depth(depths.token_accounts),
            pumpfun_fee_recipients: RwLock::new(None),
            pumpfun_fee_config: RwLock::new(None),
            pumpswap_fee_config: RwLock::new(None),
        }
    }

    /// Re-initialize fields that are not persisted so they carry the
    /// configured history depth after a load. Persisted fields are left
    /// untouched (they carry their depth inside the serialized form).
    fn reset_runtime_fields(&mut self, depths: &FieldDepths) {
        self.bonding_curves = VersionedStateMap::with_depth(depths.bonding_curves);
        self.pumpswap_pools = VersionedStateMap::with_depth(depths.pumpswap_pools);
        self.dlmm_lb_pairs = VersionedStateMap::with_depth(depths.dlmm_lb_pairs);
        self.dlmm_bin_arrays = VersionedStateMap::with_depth(depths.dlmm_bin_arrays);
        self.dlmm_bin_array_bitmap_extensions =
            VersionedStateMap::with_depth(depths.dlmm_bin_array_bitmap_extensions);
        self.dlmm_positions = VersionedStateMap::with_depth(depths.dlmm_positions);
        self.mint_token_programs = VersionedStateMap::with_depth(depths.mint_token_programs);
        self.token_accounts = VersionedStateMap::with_depth(depths.token_accounts);
        // Singletons default to None on reset — re-populated from gRPC.
        *self.pumpfun_fee_recipients.write().unwrap() = None;
        *self.pumpfun_fee_config.write().unwrap() = None;
    }
}

/// Shared metadata — lives behind an `Arc` so every cache clone shares the
/// same persist configuration, "already persisted" flag, and observed
/// slot.
#[derive(Debug)]
struct CacheMeta {
    persist_on_drop: Option<PathBuf>,
    persisted: AtomicBool,
    /// Most recent on-chain slot the cache has observed — updated by
    /// whoever is feeding the cache (the subscriber task, typically).
    /// Exposed via [`LocalCache::current_slot`] and
    /// [`LocalCache::current_slot_arc`].
    current_slot: Arc<AtomicU64>,
}

#[derive(Clone, Debug)]
pub struct LocalCache {
    data: Arc<CacheData>,
    meta: Arc<CacheMeta>,
}

impl LocalCache {
    pub fn new(config: LocalCacheConfig) -> Self {
        Self {
            data: Arc::new(CacheData::with_depths(&config.depths)),
            meta: Arc::new(CacheMeta {
                persist_on_drop: config.persist_on_drop,
                persisted: AtomicBool::new(false),
                current_slot: config
                    .current_slot
                    .unwrap_or_else(|| Arc::new(AtomicU64::new(0))),
            }),
        }
    }

    /// Try to load from `persist_on_drop`. If the path is unset, or the file
    /// does not exist, returns an empty cache. Any other error (bad magic,
    /// version mismatch, i/o) is returned.
    pub fn load_or_new(config: LocalCacheConfig) -> Result<Self, CacheError> {
        let existing_path = config
            .persist_on_drop
            .as_deref()
            .filter(|p| p.exists())
            .map(Path::to_path_buf);
        match existing_path {
            Some(path) => Self::load_from(&path, config),
            None => Ok(Self::new(config)),
        }
    }

    /// Load a snapshot from `path`. `config` supplies runtime-only state
    /// (`persist_on_drop`, the depths used to re-initialize runtime-only
    /// fields, and the shared `current_slot` atomic). Persisted fields'
    /// depths come from the serialized form.
    pub fn load_from(path: &Path, config: LocalCacheConfig) -> Result<Self, CacheError> {
        let mut data: CacheData = persist::read_payload(path)?;
        data.reset_runtime_fields(&config.depths);
        Ok(Self {
            data: Arc::new(data),
            meta: Arc::new(CacheMeta {
                persist_on_drop: config.persist_on_drop,
                persisted: AtomicBool::new(false),
                current_slot: config
                    .current_slot
                    .unwrap_or_else(|| Arc::new(AtomicU64::new(0))),
            }),
        })
    }

    /// Most recent on-chain slot the cache has observed (0 until the
    /// first writer updates the atomic).
    pub fn current_slot(&self) -> u64 {
        self.meta.current_slot.load(Ordering::Relaxed)
    }

    /// Shared handle on the slot atomic. Writers (gRPC subscriber,
    /// replay harness, …) clone this and publish updates; consumers
    /// that want a monotonic clock-style reference pass it along
    /// without re-reading the cache.
    pub fn current_slot_arc(&self) -> &Arc<AtomicU64> {
        &self.meta.current_slot
    }

    /// Publish a new observed slot. Uses `fetch_max` so late-arriving
    /// messages with older slots (reconnect snapshots, out-of-order
    /// commitment transitions) don't regress the visible slot.
    pub fn advance_slot(&self, slot: u64) {
        self.meta.current_slot.fetch_max(slot, Ordering::Relaxed);
    }

    /// Explicitly write a snapshot to the configured `persist_on_drop` path.
    pub fn save(&self) -> Result<(), CacheError> {
        let path = self
            .meta
            .persist_on_drop
            .as_deref()
            .ok_or(CacheError::NoPersistPath)?;
        self.save_to(path)
    }

    /// Write a snapshot to an arbitrary path.
    pub fn save_to(&self, path: &Path) -> Result<(), CacheError> {
        persist::write_payload(path, &*self.data)
    }

    /// `true` if this is the only outstanding handle to the cache's shared
    /// metadata (i.e. dropping this would be the last drop). Useful for
    /// shutdown coordination and tests.
    pub fn is_last_handle(&self) -> bool {
        Arc::strong_count(&self.meta) == 1
    }
}

impl Drop for LocalCache {
    fn drop(&mut self) {
        // Only the final handle attempts a persist. If two concurrent drops
        // both observe count == 2 the save is skipped by both — callers that
        // require guaranteed persistence should call `save()` explicitly
        // during shutdown. The `persisted` flag guards against double-writes
        // if a reload-and-redrop happens in the same process.
        if !self.is_last_handle() {
            return;
        }
        let Some(path) = self.meta.persist_on_drop.clone() else {
            return;
        };
        if self.meta.persisted.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Err(err) = self.save_to(&path) {
            tracing::error!(path = %path.display(), %err, "LocalCache: persist on drop failed");
        }
    }
}

// --- Access trait wiring ---------------------------------------------------

impl solana_account_traits::CacheSlot for LocalCache {
    fn current_slot(&self) -> u64 {
        LocalCache::current_slot(self)
    }
}

impl_cache_access!(LocalCache, data.blockhashes, Pubkey, [u8; 32]);
impl_cache_access!(LocalCache, data.bonding_curves, Pubkey, BondingCurve);
impl_cache_access!(LocalCache, data.pumpswap_pools, Pubkey, PumpSwapPool);
impl_cache_access!(LocalCache, data.dlmm_lb_pairs, Pubkey, LbPair);
impl_cache_access!(LocalCache, data.dlmm_bin_arrays, Pubkey, BinArray);
impl_cache_access!(
    LocalCache,
    data.dlmm_bin_array_bitmap_extensions,
    Pubkey,
    BinArrayBitmapExtension
);
impl_cache_access!(LocalCache, data.dlmm_positions, Pubkey, PositionV2Full);
impl_cache_access!(LocalCache, data.mint_token_programs, Pubkey, TokenProgram);
impl_cache_access!(LocalCache, data.token_accounts, Pubkey, TokenAccount);

impl CacheSingleton<PumpfunFeeRecipients> for LocalCache {
    fn get(&self) -> Option<PumpfunFeeRecipients> {
        *self
            .data
            .pumpfun_fee_recipients
            .read()
            .expect("pumpfun_fee_recipients lock poisoned")
    }
    fn set(&self, value: PumpfunFeeRecipients) {
        *self
            .data
            .pumpfun_fee_recipients
            .write()
            .expect("pumpfun_fee_recipients lock poisoned") = Some(value);
    }
}

impl CacheSingleton<PumpSwapFeeConfig> for LocalCache {
    fn get(&self) -> Option<PumpSwapFeeConfig> {
        self.data
            .pumpswap_fee_config
            .read()
            .expect("pumpswap_fee_config lock poisoned")
            .clone()
    }
    fn set(&self, value: PumpSwapFeeConfig) {
        *self
            .data
            .pumpswap_fee_config
            .write()
            .expect("pumpswap_fee_config lock poisoned") = Some(value);
    }
}

impl CacheSingleton<PumpfunFeeConfig> for LocalCache {
    fn get(&self) -> Option<PumpfunFeeConfig> {
        self.data
            .pumpfun_fee_config
            .read()
            .expect("pumpfun_fee_config lock poisoned")
            .clone()
    }
    fn set(&self, value: PumpfunFeeConfig) {
        *self
            .data
            .pumpfun_fee_config
            .write()
            .expect("pumpfun_fee_config lock poisoned") = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_account_traits::{CacheGet, CacheInsert};
    use solana_protocols::parsing::state::Legacy;
    use tempfile::tempdir;

    fn pk(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn hash(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn new_cache_is_empty_and_roundtrips_through_the_access_traits() {
        let cache = LocalCache::new(LocalCacheConfig::default());
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&cache, &pk(1)),
            None
        );

        <LocalCache as CacheInsert<Pubkey, [u8; 32]>>::insert(&cache, pk(1), hash(0xab), 100);
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&cache, &pk(1)),
            Some(hash(0xab))
        );
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get_with_slot(&cache, &pk(1)),
            Some((hash(0xab), 100))
        );
    }

    #[test]
    fn clone_shares_state_across_handles() {
        let a = LocalCache::new(LocalCacheConfig::default());
        let b = a.clone();
        <LocalCache as CacheInsert<Pubkey, [u8; 32]>>::insert(&a, pk(1), hash(0x11), 100);
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&b, &pk(1)),
            Some(hash(0x11))
        );
        assert!(!a.is_last_handle());
        drop(b);
        assert!(a.is_last_handle());
    }

    #[test]
    fn save_and_load_roundtrip_preserves_data_and_history() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.bin.gz");

        let cache = LocalCache::new(LocalCacheConfig::default());
        cache.insert(pk(1), hash(0x10), 100);
        cache.insert(pk(1), hash(0x11), 101);
        cache.insert(pk(2), hash(0x20), 100);
        cache.save_to(&path).expect("save");

        let restored = LocalCache::load_from(&path, LocalCacheConfig::default()).expect("load");
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&restored, &pk(1)),
            Some(hash(0x11))
        );
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get_at_slot(&restored, &pk(1), 100),
            Some(hash(0x10))
        );
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&restored, &pk(2)),
            Some(hash(0x20))
        );
    }

    #[test]
    fn save_without_persist_path_errors_but_save_to_works() {
        let cache = LocalCache::new(LocalCacheConfig::default());
        matches!(cache.save(), Err(CacheError::NoPersistPath));

        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.bin.gz");
        cache.save_to(&path).expect("save_to");
        assert!(path.exists());
    }

    #[test]
    fn load_or_new_on_missing_path_returns_empty_cache() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.bin.gz");
        let cache = LocalCache::load_or_new(LocalCacheConfig {
            persist_on_drop: Some(path.clone()),
            ..Default::default()
        })
        .expect("load_or_new");
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&cache, &pk(1)),
            None
        );
        assert!(!path.exists());
    }

    #[test]
    fn load_from_bad_magic_file_errors_cleanly() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.bin.gz");
        std::fs::write(&path, b"not a solana-account-cache snapshot").unwrap();
        let err = LocalCache::load_from(&path, LocalCacheConfig::default()).unwrap_err();
        matches!(err, CacheError::BadMagic { .. });
    }

    #[test]
    fn load_from_wrong_version_errors_with_expected_and_found() {
        use crate::persist::MAGIC;
        let dir = tempdir().unwrap();
        let path = dir.path().join("wrongver.bin.gz");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&999u32.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        match LocalCache::load_from(&path, LocalCacheConfig::default()) {
            Err(CacheError::UnsupportedVersion {
                expected: _,
                found: 999,
                ..
            }) => {}
            other => panic!("expected UnsupportedVersion{{found: 999}}, got {other:?}"),
        }
    }

    #[test]
    fn drop_persists_on_last_handle_and_skips_on_non_last() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("drop.bin.gz");

        let a = LocalCache::new(LocalCacheConfig {
            persist_on_drop: Some(path.clone()),
            ..Default::default()
        });
        let b = a.clone();
        a.insert(pk(7), hash(0x77), 500);

        drop(a);
        assert!(!path.exists(), "non-last drop persisted unexpectedly");

        drop(b);
        assert!(path.exists(), "last drop did not persist");

        let restored = LocalCache::load_from(&path, LocalCacheConfig::default()).expect("load");
        assert_eq!(
            <LocalCache as CacheGet<Pubkey, [u8; 32]>>::get(&restored, &pk(7)),
            Some(hash(0x77))
        );
    }

    #[test]
    fn drop_without_persist_path_is_a_noop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("unused.bin.gz");
        {
            let cache = LocalCache::new(LocalCacheConfig::default());
            cache.insert(pk(1), hash(0x01), 100);
        }
        assert!(!path.exists());
    }

    #[test]
    fn current_slot_starts_at_zero_and_advances_monotonically() {
        let cache = LocalCache::new(LocalCacheConfig::default());
        assert_eq!(cache.current_slot(), 0);
        cache.advance_slot(100);
        assert_eq!(cache.current_slot(), 100);
        // fetch_max semantics — an older slot doesn't regress the value.
        cache.advance_slot(50);
        assert_eq!(cache.current_slot(), 100);
        cache.advance_slot(101);
        assert_eq!(cache.current_slot(), 101);
    }

    #[test]
    fn external_current_slot_arc_is_shared_between_cache_and_caller() {
        // Simulates the subscriber-owns-writer / cache-owns-reader pattern:
        // a caller holds a clone of the same atomic we configure the cache
        // with; updates from either side are visible to the other.
        let external = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let cache = LocalCache::new(LocalCacheConfig {
            current_slot: Some(std::sync::Arc::clone(&external)),
            ..Default::default()
        });

        external.store(42, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(cache.current_slot(), 42);

        cache.advance_slot(43);
        assert_eq!(external.load(std::sync::atomic::Ordering::Relaxed), 43);

        // cache handle exposes the same Arc back out for consumers that
        // want to share it further.
        assert!(std::sync::Arc::ptr_eq(cache.current_slot_arc(), &external));
    }

    // --- pump_fees FeeConfig family: pubkey routing ------------------------

    /// Borsh-encode a `FeeConfig` with a recognisable `bump`, behind the
    /// `account:FeeConfig` discriminator both PDAs carry.
    fn encode_fee_config(bump: u8) -> Vec<u8> {
        use solana_protocols::pumpfun::FEE_CONFIG_DISCRIMINATOR;
        let mut d = Vec::new();
        d.extend_from_slice(&FEE_CONFIG_DISCRIMINATOR);
        d.push(bump);
        d.extend_from_slice(&[7u8; 32]); // admin
        for _ in 0..3 {
            d.extend_from_slice(&0u64.to_le_bytes()); // flat_fees
        }
        d.extend_from_slice(&0u32.to_le_bytes()); // fee_tiers: empty vec
        d
    }

    /// A PumpSwap FeeConfig update must **not** land in pumpfun's slot.
    ///
    /// Both PDAs are owned by `pump_fees` and carry the same
    /// `account:FeeConfig` discriminator — the registry's whole dispatch key —
    /// so one handler receives both. Before `apply` routed on the pubkey, the
    /// second update overwrote the first and pumpfun quoting silently read
    /// PumpSwap's fee tiers. Reverting that routing turns this test red.
    #[test]
    fn pumpswap_fee_config_does_not_overwrite_pumpfun_singleton() {
        use solana_account_traits::{CacheSingleton, HandlerRegistry};
        use solana_protocols::pumpfun::handler::PumpfunFeeConfigHandler;
        use solana_protocols::pumpfun::{FEE_CONFIG_PDA, PUMP_FEES_PROGRAM_ID};
        use solana_protocols::pumpswap::FEE_CONFIG as PUMPSWAP_FEE_CONFIG;

        let cache = LocalCache::new(LocalCacheConfig::default());
        let mut registry: HandlerRegistry<LocalCache> = HandlerRegistry::new();
        registry.register(PumpfunFeeConfigHandler::new());

        // pumpfun's config lands in pumpfun's slot.
        registry
            .dispatch(
                &cache,
                &PUMP_FEES_PROGRAM_ID,
                &FEE_CONFIG_PDA,
                &encode_fee_config(11),
                100,
            )
            .expect("pumpfun fee config dispatch");
        let pumpfun: Option<PumpfunFeeConfig> = CacheSingleton::get(&cache);
        assert_eq!(pumpfun.expect("pumpfun slot populated").bump, 11);

        // PumpSwap's config arrives through the *same* handler...
        registry
            .dispatch(
                &cache,
                &PUMP_FEES_PROGRAM_ID,
                &PUMPSWAP_FEE_CONFIG,
                &encode_fee_config(22),
                101,
            )
            .expect("pumpswap fee config dispatch");

        // ...and must land in its own slot, leaving pumpfun's untouched.
        let pumpfun: Option<PumpfunFeeConfig> = CacheSingleton::get(&cache);
        assert_eq!(
            pumpfun.expect("pumpfun slot still populated").bump,
            11,
            "PumpSwap's FeeConfig overwrote pumpfun's singleton"
        );
        let pumpswap: Option<PumpSwapFeeConfig> = CacheSingleton::get(&cache);
        assert_eq!(pumpswap.expect("pumpswap slot populated").0.bump, 22);
    }

    /// A FeeConfig for an unmodelled consuming program is skipped, not written
    /// over either slot we do model.
    #[test]
    fn unrouted_fee_config_pda_overwrites_nothing() {
        use solana_account_traits::{CacheSingleton, HandlerRegistry};
        use solana_protocols::pumpfun::handler::PumpfunFeeConfigHandler;
        use solana_protocols::pumpfun::{FEE_CONFIG_PDA, PUMP_FEES_PROGRAM_ID};

        let cache = LocalCache::new(LocalCacheConfig::default());
        let mut registry: HandlerRegistry<LocalCache> = HandlerRegistry::new();
        registry.register(PumpfunFeeConfigHandler::new());

        registry
            .dispatch(
                &cache,
                &PUMP_FEES_PROGRAM_ID,
                &FEE_CONFIG_PDA,
                &encode_fee_config(11),
                100,
            )
            .unwrap();
        registry
            .dispatch(
                &cache,
                &PUMP_FEES_PROGRAM_ID,
                &pk(0xEE), // some third program's FeeConfig PDA
                &encode_fee_config(99),
                101,
            )
            .expect("unmodelled PDA still dispatches cleanly");

        let pumpfun: Option<PumpfunFeeConfig> = CacheSingleton::get(&cache);
        assert_eq!(pumpfun.expect("populated").bump, 11);
        let pumpswap: Option<PumpSwapFeeConfig> = CacheSingleton::get(&cache);
        assert!(pumpswap.is_none(), "unmodelled PDA must not fill a slot");
    }

    // --- Phase 6 end-to-end: Pumpfun handler → LocalCache ------------------

    /// Encodes the **81-byte legacy layout**, which predates the cashback
    /// flags — so a curve round-tripped through it must carry
    /// `Legacy::Absent`, not `Present(false)`. The two are different facts and
    /// the decode is right to distinguish them.
    fn encode_bonding_curve(curve: &BondingCurve) -> Vec<u8> {
        use solana_protocols::pumpfun::BONDING_CURVE_DISCRIMINATOR;
        let mut data = Vec::with_capacity(81);
        data.extend_from_slice(&BONDING_CURVE_DISCRIMINATOR);
        data.extend_from_slice(&curve.virtual_token_reserves.to_le_bytes());
        data.extend_from_slice(&curve.virtual_quote_reserves.to_le_bytes());
        data.extend_from_slice(&curve.real_token_reserves.to_le_bytes());
        data.extend_from_slice(&curve.real_quote_reserves.to_le_bytes());
        data.extend_from_slice(&curve.token_total_supply.to_le_bytes());
        data.push(if curve.complete { 1 } else { 0 });
        data.extend_from_slice(curve.creator.as_ref());
        data
    }

    fn sample_bonding_curve() -> BondingCurve {
        BondingCurve {
            is_mayhem_mode: Legacy::Absent,
            is_cashback_coin: Legacy::Absent,
            virtual_token_reserves: 1_000_000_000_000_000,
            virtual_quote_reserves: 30_000_000_000,
            real_token_reserves: 800_000_000_000_000,
            real_quote_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
            creator: pk(0xCC),
        }
    }

    #[test]
    fn pumpfun_handler_parses_account_update_and_writes_into_local_cache() {
        use solana_account_traits::HandlerRegistry;
        use solana_protocols::pumpfun::{handler::PumpfunBondingCurveHandler, PROGRAM_ID};

        let cache = LocalCache::new(LocalCacheConfig::default());
        let mut registry: HandlerRegistry<LocalCache> = HandlerRegistry::new();
        registry.register(PumpfunBondingCurveHandler::new());

        let curve = sample_bonding_curve();
        let data = encode_bonding_curve(&curve);
        let curve_pubkey = pk(0x42);

        let result = registry
            .dispatch(&cache, &PROGRAM_ID, &curve_pubkey, &data, 500)
            .expect("dispatch succeeds");

        // Handler derived the spot price from the state it just parsed.
        assert_eq!(result.spot_price, Some(curve.spot_price()));
        // No declared deps on a bare bonding-curve update.
        assert!(result.accounts_to_fetch.is_empty());
        assert!(result.accounts_to_subscribe.is_empty());

        // Cache now contains the parsed BondingCurve, reachable through the
        // generic CacheGet<Pubkey, BondingCurve> bound the handler wrote
        // against — no direct reference to LocalCache from the handler.
        let stored = <LocalCache as CacheGet<Pubkey, BondingCurve>>::get(&cache, &curve_pubkey)
            .expect("bonding curve present in cache");
        assert_eq!(stored, curve);

        // Slot is preserved.
        let with_slot =
            <LocalCache as CacheGet<Pubkey, BondingCurve>>::get_with_slot(&cache, &curve_pubkey);
        assert_eq!(with_slot, Some((curve, 500)));
    }

    #[test]
    fn pumpfun_handler_rejects_wrong_discriminator_via_registry() {
        use solana_account_traits::{HandlerError, HandlerRegistry};
        use solana_protocols::pumpfun::{handler::PumpfunBondingCurveHandler, PROGRAM_ID};

        let cache = LocalCache::new(LocalCacheConfig::default());
        let mut registry: HandlerRegistry<LocalCache> = HandlerRegistry::new();
        registry.register(PumpfunBondingCurveHandler::new());

        // BondingCurve dispatch is gated on the Anchor discriminator.
        // A large zero-filled buffer carries the wrong first 8 bytes,
        // so the registry reports NoHandler — regardless of size.
        let data = vec![0u8; 151];
        let err = registry
            .dispatch(&cache, &PROGRAM_ID, &pk(0x42), &data, 500)
            .unwrap_err();
        assert!(matches!(err, HandlerError::NoHandler { .. }));
    }

    #[test]
    fn pumpfun_handler_advances_history_on_subsequent_slot_updates() {
        use solana_account_traits::HandlerRegistry;
        use solana_protocols::pumpfun::{handler::PumpfunBondingCurveHandler, PROGRAM_ID};

        let cache = LocalCache::new(LocalCacheConfig::default());
        let mut registry: HandlerRegistry<LocalCache> = HandlerRegistry::new();
        registry.register(PumpfunBondingCurveHandler::new());

        let mut curve = sample_bonding_curve();
        let curve_pubkey = pk(0x42);

        // First update at slot 500.
        let data = encode_bonding_curve(&curve);
        registry
            .dispatch(&cache, &PROGRAM_ID, &curve_pubkey, &data, 500)
            .expect("dispatch 1");

        // Reserves move (buy happened), later slot.
        curve.virtual_quote_reserves += 1_000_000_000;
        let data = encode_bonding_curve(&curve);
        registry
            .dispatch(&cache, &PROGRAM_ID, &curve_pubkey, &data, 501)
            .expect("dispatch 2");

        // Current is the new state, slot 500 still reachable via history.
        let now =
            <LocalCache as CacheGet<Pubkey, BondingCurve>>::get(&cache, &curve_pubkey).unwrap();
        assert_eq!(now.virtual_quote_reserves, 31_000_000_000);

        let then =
            <LocalCache as CacheGet<Pubkey, BondingCurve>>::get_at_slot(&cache, &curve_pubkey, 500)
                .unwrap();
        assert_eq!(then.virtual_quote_reserves, 30_000_000_000);
    }

    #[test]
    fn singleton_traits_roundtrip_through_local_cache() {
        use solana_account_traits::CacheSingleton;
        use solana_protocols::pumpfun::{
            PumpfunFeeConfig, PumpfunFeeRecipients, PumpfunFeeTier, PumpfunFees,
        };

        let cache = LocalCache::new(LocalCacheConfig::default());

        // PumpfunFeeRecipients singleton roundtrip.
        assert_eq!(
            <LocalCache as CacheSingleton<PumpfunFeeRecipients>>::get(&cache),
            None
        );
        let recipients = PumpfunFeeRecipients {
            regular: pk(0xAA),
            mayhem: pk(0xBB),
        };
        <LocalCache as CacheSingleton<PumpfunFeeRecipients>>::set(&cache, recipients);
        assert_eq!(
            <LocalCache as CacheSingleton<PumpfunFeeRecipients>>::get(&cache),
            Some(recipients)
        );

        // PumpfunFeeConfig singleton roundtrip.
        let fee_config = PumpfunFeeConfig {
            bump: 7,
            admin: pk(0x01),
            flat_fees: PumpfunFees {
                lp_fee_bps: 0,
                protocol_fee_bps: 95,
                creator_fee_bps: 30,
            },
            fee_tiers: vec![PumpfunFeeTier {
                market_cap_lamports_threshold: 1_000_000,
                fees: PumpfunFees {
                    lp_fee_bps: 0,
                    protocol_fee_bps: 100,
                    creator_fee_bps: 50,
                },
            }],
        };
        <LocalCache as CacheSingleton<PumpfunFeeConfig>>::set(&cache, fee_config.clone());
        let round = <LocalCache as CacheSingleton<PumpfunFeeConfig>>::get(&cache).unwrap();
        assert_eq!(round.bump, fee_config.bump);
        assert_eq!(round.fee_tiers.len(), 1);
        assert_eq!(round.fee_tiers[0].fees.protocol_fee_bps, 100);
    }

    #[test]
    fn singleton_state_clones_across_handles() {
        use solana_account_traits::CacheSingleton;
        use solana_protocols::pumpfun::PumpfunFeeRecipients;

        let a = LocalCache::new(LocalCacheConfig::default());
        let b = a.clone();
        let recipients = PumpfunFeeRecipients {
            regular: pk(0x10),
            mayhem: pk(0x11),
        };
        <LocalCache as CacheSingleton<PumpfunFeeRecipients>>::set(&a, recipients);
        // Cloned handle sees the same singleton via shared Arc.
        assert_eq!(
            <LocalCache as CacheSingleton<PumpfunFeeRecipients>>::get(&b),
            Some(recipients)
        );
    }
}
