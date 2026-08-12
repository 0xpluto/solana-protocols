//! `solana-account-traits` — the trait contracts that sit between storage backends
//! and protocol code.
//!
//! This crate has **zero knowledge** of any concrete cache or any concrete
//! protocol. It defines:
//!
//! * [`CacheGet`] / [`CacheInsert`] — the access contracts downstream code
//!   binds against so it never has to import a concrete cache crate.
//! * [`ProtocolStateHandler`] / [`StorageHandler`] / [`ErasedHandler`] /
//!   [`HandlerRegistry`] — the handler framework. `StorageHandler<C>` and
//!   `ErasedHandler<C>` are generic over the cache type, so protocol
//!   handler implementations stay cache-agnostic — they only bind
//!   `C: CacheInsert<K, V>` for whichever fields they write to.
//! * [`InsertOutcome`] — the shared enum returned by every insert.
//!
//! Protocol crates implement `StorageHandler<C>` generically. Cache crates
//! (e.g. `solana-account-cache`) provide the concrete `C` and the `CacheInsert` /
//! `CacheGet` impls. Neither depends on the other.

mod access;
mod handler;

pub use access::{CacheGet, CacheInsert, CacheSingleton, CacheSlot, InsertOutcome};
pub use handler::{
    DeliveryExpectation, Dependency, ErasedHandler, HandleResult, HandlerError, HandlerRegistry,
    ProtocolStateHandler, StorageHandler, VerifiedDecoder,
};
