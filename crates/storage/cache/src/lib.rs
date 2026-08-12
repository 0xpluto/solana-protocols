//! `solana-account-cache` — slot-versioned in-memory mirror of on-chain account state.
//!
//! Concrete cache implementation. Trait contracts ([`CacheGet`],
//! [`CacheInsert`], [`ProtocolStateHandler`], [`StorageHandler`],
//! [`HandlerRegistry`], …) live in the `solana-account-traits` crate — import them
//! from there directly.
//!
//! This crate exposes:
//!
//! * [`VersionedStateMap`] — the concurrent, slot-versioned primitive the
//!   cache is built on.
//! * [`LocalCache`] — the composed, clone-able cache with gzip-bincode
//!   persistence and last-handle drop save. Field layout lives in a
//!   private `CacheData` struct; new fields are added there.
//!
//! [`CacheGet`]: solana_account_traits::CacheGet
//! [`CacheInsert`]: solana_account_traits::CacheInsert
//! [`ProtocolStateHandler`]: solana_account_traits::ProtocolStateHandler
//! [`StorageHandler`]: solana_account_traits::StorageHandler
//! [`HandlerRegistry`]: solana_account_traits::HandlerRegistry

mod cache;
mod persist;
mod versioned_map;

pub use cache::{FieldDepths, LocalCache, LocalCacheConfig};
pub use persist::CacheError;
pub use versioned_map::{StateWithSlot, VersionedState, VersionedStateMap};
