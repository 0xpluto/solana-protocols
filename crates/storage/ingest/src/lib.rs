//! `solana-account-ingest` — normalize account updates from any source (RPC batch
//! fetch, gRPC stream) and drive them through a [`HandlerRegistry`].
//!
//! This crate defines the shape of an account update
//! ([`AccountUpdate`]), the [`AccountFetcher`] trait that pull-style
//! sources (RPC) implement, and the [`Ingest`] loop that dispatches one
//! update through the registry. Dependencies a handler reports in its
//! [`HandleResult`](solana_account_traits::HandleResult) are resolved off the hot
//! path by a [`DepResolver`], so dispatch never blocks on I/O.
//!
//! Concrete sources (Solana RPC, Yellowstone gRPC) are provided elsewhere
//! — this crate depends only on `solana-account-traits` and `solana-pubkey`, so
//! it stays generic over any cache `C` and any fetcher `F`.
//!
//! [`HandlerRegistry`]: solana_account_traits::HandlerRegistry

mod account_update;
mod fetcher;
mod ingest;

#[cfg(feature = "rpc")]
mod rpc;

pub use account_update::AccountUpdate;
pub use fetcher::{AccountFetcher, FetchError};
pub use ingest::{DepResolver, Ingest, IngestError};

#[cfg(feature = "rpc")]
pub use rpc::RpcFetcher;
