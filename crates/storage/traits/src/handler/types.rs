//! Shared types passed across the handler boundary.

use solana_pubkey::Pubkey;

/// How a dependency reaches us — which decides subscribe-vs-fetch.
///
/// This is about **delivery**, not merely cadence: the distinguishing question
/// is whether the account is already covered by a wholesale owner
/// subscription, needs its own per-pubkey subscription, or will not arrive on
/// any stream in useful time.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum DeliveryExpectation {
    /// Config-style accounts (fee configs, globals): owned by a subscribed
    /// program but written so rarely that waiting for the next update could
    /// take hours — and quote math needs them now. **The only class that is
    /// RPC-fetched**, and that fetch belongs on a background worker, never on
    /// the account hot path.
    Infrequent,
    /// Program-owned state that updates constantly (pool accounts, bonding
    /// curves). Already covered by the wholesale owner subscription, so it
    /// needs neither a fetch nor a new subscription — it is already on its way.
    Frequent,
    /// Accounts **not owned by the protocol's program**, so no owner filter
    /// covers them — a token vault is the canonical case. These must be
    /// *dynamically subscribed by pubkey*. Never RPC-fetched: the subscription
    /// is the delivery mechanism, and fetching one per update puts a blocking
    /// round-trip on the hot path.
    Dynamic,
}

/// An account that a handler needs in order to do its job.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Dependency {
    pub pubkey: Pubkey,
    pub expectation: DeliveryExpectation,
}

impl Dependency {
    pub fn new(pubkey: Pubkey, expectation: DeliveryExpectation) -> Self {
        Self {
            pubkey,
            expectation,
        }
    }
}

/// What a handler reports to the ingest layer after applying an update.
#[derive(Debug, Default, Clone)]
pub struct HandleResult {
    /// Accounts needed now — ingest should RPC-fetch them immediately.
    pub accounts_to_fetch: Vec<Dependency>,
    /// Accounts expected via already-active gRPC subscriptions. No new work.
    pub accounts_pending_grpc: Vec<Dependency>,
    /// Accounts that need a new gRPC subscription (and typically an RPC
    /// bootstrap while the stream warms up).
    pub accounts_to_subscribe: Vec<Dependency>,
    /// Spot price derived from this update, in handler-defined units.
    pub spot_price: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("failed to deserialize account data (len={data_len}): {reason}")]
    Deserialize { data_len: usize, reason: String },

    #[error("no handler registered for program {program_id} (first 8 bytes: {discriminator:?})")]
    NoHandler {
        program_id: Pubkey,
        discriminator: Option<[u8; 8]>,
    },

    #[error("apply failed for account {pubkey}: {reason}")]
    Apply { pubkey: Pubkey, reason: String },
}
