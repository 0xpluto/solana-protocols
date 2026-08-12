//! Handler framework.
//!
//! Three trait layers:
//!
//! * [`ProtocolStateHandler`] — parse-only. Program ID, discriminator,
//!   `matches_account`, `deserialize`. No cache knowledge.
//! * [`StorageHandler<C>`] — extends the above with `apply(&C, …)`. Generic
//!   over the cache type so protocol implementors only bind
//!   `C: CacheInsert<K, V>` for whichever fields they write to.
//! * [`ErasedHandler<C>`] — object-safe facade the registry stores.
//!   Implemented blanket-style for every `StorageHandler<C>`.
//!
//! [`HandlerRegistry<C>`] owns dispatch — O(1) for Anchor accounts (8-byte
//! discriminator), linear fallback for non-Anchor accounts via
//! `matches_account`.

mod registry;
mod traits;
mod types;

pub use registry::{ErasedHandler, HandlerRegistry};
pub use traits::{ProtocolStateHandler, StorageHandler, VerifiedDecoder};
pub use types::{DeliveryExpectation, Dependency, HandleResult, HandlerError};
