//! `Ingest<C, F>` — synchronous dispatch, with dependency resolution moved off
//! the hot path.
//!
//! [`Ingest::apply`] takes an [`AccountUpdate`], hands it to the
//! [`HandlerRegistry`], and returns. It performs **no I/O and no `await`** —
//! an account update must return as soon as it is applied, because the caller
//! is draining a gRPC stream and anything blocking there is backpressure on
//! every account for every protocol.
//!
//! Dependencies a handler reports are routed by [`DeliveryExpectation`], not by
//! which list they arrived in (see [`fetch_targets`]):
//!
//! * `Frequent` / `Dynamic` → nothing to do here; the subscription delivers them.
//! * `Infrequent` → queued to a [`DepResolver`], which fetches them on its own
//!   task and feeds the results back through `apply`.
//!
//! The resolver is handed to the caller at construction rather than spawned
//! internally: this crate provides the loop, the caller owns the runtime it
//! runs on.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use solana_account_traits::{DeliveryExpectation, HandleResult, HandlerError, HandlerRegistry};
use solana_pubkey::Pubkey;
use tokio::sync::mpsc;
use tracing::warn;

use crate::{AccountFetcher, AccountUpdate};

/// Depth of the queue between the hot path and the resolver.
///
/// Only `Infrequent` (config) accounts travel it, and the resolver dedupes, so
/// steady-state depth is ~0. A full queue therefore means the resolver is
/// wedged; dropping is the right response — the dependency is re-reported on
/// the account's next update, whereas blocking would stall ingestion.
const DEP_QUEUE_CAPACITY: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("handler error on primary update: {0}")]
    PrimaryHandler(#[from] HandlerError),
}

pub struct Ingest<C: 'static, F: AccountFetcher> {
    registry: HandlerRegistry<C>,
    cache: C,
    fetcher: F,
    deps_tx: mpsc::Sender<Pubkey>,
    deps_dropped: AtomicU64,
}

impl<C, F> Ingest<C, F>
where
    C: Send + Sync + 'static,
    F: AccountFetcher,
{
    /// Build the dispatcher **and** the worker that resolves its `Infrequent`
    /// dependencies.
    ///
    /// The two are returned together because they are one thing: without
    /// spawning [`DepResolver::run`], config dependencies are queued and never
    /// loaded. Handing the worker back is what makes that impossible to forget
    /// — there is no way to obtain an `Ingest` without also holding its
    /// resolver.
    pub fn new(
        registry: HandlerRegistry<C>,
        cache: C,
        fetcher: F,
    ) -> (Arc<Self>, DepResolver<C, F>) {
        let (deps_tx, deps_rx) = mpsc::channel(DEP_QUEUE_CAPACITY);
        let ingest = Arc::new(Self {
            registry,
            cache,
            fetcher,
            deps_tx,
            deps_dropped: AtomicU64::new(0),
        });
        let resolver = DepResolver {
            ingest: Arc::clone(&ingest),
            deps_rx,
            seen: HashSet::new(),
        };
        (ingest, resolver)
    }

    pub fn registry(&self) -> &HandlerRegistry<C> {
        &self.registry
    }

    pub fn cache(&self) -> &C {
        &self.cache
    }

    /// Dependency requests dropped because the resolver queue was full.
    ///
    /// Non-zero means config accounts may be stale — surface it as a gauge.
    pub fn deps_dropped(&self) -> u64 {
        self.deps_dropped.load(Ordering::Relaxed)
    }

    /// Dispatch `update` through the registry, queue any `Infrequent`
    /// dependencies for background resolution, and return.
    ///
    /// Synchronous by design: no `await`, no I/O, no lock held across a yield.
    /// Errors are the dispatch's own (no handler, bad decode, handler failure).
    pub fn apply(&self, update: AccountUpdate) -> Result<HandleResult, IngestError> {
        let result = self.registry.dispatch(
            &self.cache,
            &update.owner,
            &update.pubkey,
            &update.data,
            update.slot,
        )?;
        for pubkey in fetch_targets(&result) {
            // `try_send`, never `send`: the whole point is that this call
            // cannot block the stream. See DEP_QUEUE_CAPACITY.
            if self.deps_tx.try_send(pubkey).is_err() {
                self.deps_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(result)
    }
}

/// Resolves the `Infrequent` dependencies [`Ingest::apply`] queues.
///
/// Owns the only RPC path in this crate. Spawn [`run`](Self::run) on its own
/// task; it exits when the last `Ingest` handle is dropped.
pub struct DepResolver<C: 'static, F: AccountFetcher> {
    ingest: Arc<Ingest<C, F>>,
    deps_rx: mpsc::Receiver<Pubkey>,
    /// Already fetched — a dependency resolves **once**, not once per update
    /// of the account that reports it. Without this, a handler naming its
    /// config on every update turns into one RPC per update. Bounded in
    /// practice: this class is per-protocol config accounts, a handful total.
    seen: HashSet<Pubkey>,
}

impl<C, F> DepResolver<C, F>
where
    C: Send + Sync + 'static,
    F: AccountFetcher,
{
    /// Resolve queued dependencies until every `Ingest` handle is dropped.
    pub async fn run(mut self) {
        while let Some(pubkey) = self.deps_rx.recv().await {
            self.resolve(pubkey).await;
        }
    }

    /// Resolve everything queued *right now*, then return.
    ///
    /// Deterministic counterpart to [`run`](Self::run) for tests and for
    /// callers that want a warm cache before starting the stream.
    pub async fn drain(&mut self) {
        while let Ok(pubkey) = self.deps_rx.try_recv() {
            self.resolve(pubkey).await;
        }
    }

    async fn resolve(&mut self, pubkey: Pubkey) {
        if !self.seen.insert(pubkey) {
            return;
        }
        match self.ingest.fetcher.fetch(&[pubkey]).await {
            Ok(updates) => {
                for update in updates.into_iter().flatten() {
                    // Re-entering `apply` also re-queues any transitive deps.
                    if let Err(err) = self.ingest.apply(update) {
                        warn!(%pubkey, %err, "ingest: dependency dispatch failed");
                    }
                }
            }
            Err(err) => {
                // Un-see it: a transient RPC failure must not blacklist the
                // account forever. The next update re-reports it.
                self.seen.remove(&pubkey);
                warn!(%pubkey, %err, "ingest: dependency fetch failed");
            }
        }
    }
}

/// Dependencies that warrant an **RPC fetch**, honouring
/// [`DeliveryExpectation`] — the axis that exists exactly to answer this.
///
/// * [`Infrequent`] → **the only class fetched.** Config accounts are owned by
///   a subscribed program but written so rarely that the next update may be
///   hours away, and quote math needs them now.
/// * [`Frequent`] → never fetched: program-owned state already covered by the
///   wholesale owner subscription. It is already on its way.
/// * [`Dynamic`] → never fetched: not owned by the protocol's program, so it is
///   *dynamically subscribed by pubkey* instead. Fetching this class is what
///   collapsed ingestion ~20× (56,925 accounts shed in 75s) when pool handlers
///   began reporting their vaults — one blocking RPC per dependency per update,
///   un-deduplicated, on the account hot path.
///
/// [`Frequent`]: DeliveryExpectation::Frequent
/// [`Infrequent`]: DeliveryExpectation::Infrequent
/// [`Dynamic`]: DeliveryExpectation::Dynamic
fn fetch_targets(result: &HandleResult) -> impl Iterator<Item = Pubkey> + '_ {
    result
        .accounts_to_fetch
        .iter()
        .chain(result.accounts_to_subscribe.iter())
        .filter(|d| d.expectation == DeliveryExpectation::Infrequent)
        .map(|d| d.pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use solana_account_traits::{Dependency, ProtocolStateHandler, StorageHandler};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    use crate::FetchError;

    // --- Stub fetcher: canned responses per-pubkey + toggleable failure ---

    #[derive(Default)]
    struct StubFetcher {
        responses: Mutex<HashMap<Pubkey, AccountUpdate>>,
        fail: AtomicBool,
        calls: Mutex<Vec<Vec<Pubkey>>>,
    }

    impl StubFetcher {
        fn new() -> Self {
            Self::default()
        }
        fn add(&self, update: AccountUpdate) {
            self.responses.lock().unwrap().insert(update.pubkey, update);
        }
        fn set_fail(&self, v: bool) {
            self.fail.store(v, Ordering::SeqCst);
        }
        fn calls(&self) -> Vec<Vec<Pubkey>> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl AccountFetcher for StubFetcher {
        async fn fetch(
            &self,
            pubkeys: &[Pubkey],
        ) -> Result<Vec<Option<AccountUpdate>>, FetchError> {
            self.calls.lock().unwrap().push(pubkeys.to_vec());
            if self.fail.load(Ordering::SeqCst) {
                return Err(FetchError::Backend("stub fail".into()));
            }
            let responses = self.responses.lock().unwrap();
            Ok(pubkeys
                .iter()
                .map(|pk| responses.get(pk).cloned())
                .collect())
        }
    }

    // --- Stub handler: returns a canned HandleResult and records calls.
    // Cache-agnostic via the generic StorageHandler<C> impl.

    #[derive(Default)]
    struct DispatchLog {
        calls: Mutex<Vec<Pubkey>>,
    }

    struct ConfiguredHandler {
        program_id: Pubkey,
        discriminator: &'static [u8; 8],
        response: HandleResult,
        log: Arc<DispatchLog>,
    }

    impl ProtocolStateHandler for ConfiguredHandler {
        type State = ();
        fn program_id(&self) -> Pubkey {
            self.program_id
        }
        fn discriminator(&self) -> Option<&'static [u8]> {
            Some(self.discriminator.as_slice())
        }
        fn deserialize(&self, _: &[u8]) -> Result<Self::State, HandlerError> {
            Ok(())
        }
    }

    impl<C> StorageHandler<C> for ConfiguredHandler
    where
        C: Send + Sync + 'static,
    {
        fn apply(
            &self,
            _cache: &C,
            pubkey: &Pubkey,
            _state: &Self::State,
            _slot: u64,
        ) -> Result<HandleResult, HandlerError> {
            self.log.calls.lock().unwrap().push(*pubkey);
            Ok(self.response.clone())
        }
    }

    fn pk(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn disc_data(disc: &[u8; 8]) -> Vec<u8> {
        let mut v = disc.to_vec();
        v.extend_from_slice(&[0u8; 16]); // pad so data.len() >= 8
        v
    }

    fn update_for(owner: Pubkey, pk: Pubkey, disc: &[u8; 8], slot: u64) -> AccountUpdate {
        AccountUpdate {
            pubkey: pk,
            owner,
            data: disc_data(disc),
            slot,
        }
    }

    const DISC_A: &[u8; 8] = b"HANDLR_A";
    const DISC_B: &[u8; 8] = b"HANDLR_B";
    const DISC_C: &[u8; 8] = b"HANDLR_C";

    /// Builds an ingest whose registry holds `handlers`, plus its resolver.
    fn make_ingest(
        handlers: Vec<(Pubkey, &'static [u8; 8], HandleResult, Arc<DispatchLog>)>,
    ) -> (Arc<Ingest<(), StubFetcher>>, DepResolver<(), StubFetcher>) {
        let mut registry: HandlerRegistry<()> = HandlerRegistry::new();
        for (program_id, disc, response, log) in handlers {
            registry.register(ConfiguredHandler {
                program_id,
                discriminator: disc,
                response,
                log,
            });
        }
        Ingest::new(registry, (), StubFetcher::new())
    }

    /// A dep-reporting handler paired with a handler for the dep itself.
    fn ingest_with_dep(
        primary: HandleResult,
        primary_log: Arc<DispatchLog>,
        dep_log: Arc<DispatchLog>,
    ) -> (Arc<Ingest<(), StubFetcher>>, DepResolver<(), StubFetcher>) {
        make_ingest(vec![
            (pk(0xA0), DISC_A, primary, primary_log),
            (pk(0xB0), DISC_B, HandleResult::default(), dep_log),
        ])
    }

    #[tokio::test]
    async fn apply_dispatches_single_update_and_returns_its_handle_result() {
        let log = Arc::new(DispatchLog::default());
        let response = HandleResult {
            spot_price: Some(1.25),
            ..Default::default()
        };
        let (ingest, _resolver) = make_ingest(vec![(pk(0xA0), DISC_A, response, log.clone())]);

        let result = ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply");
        assert_eq!(result.spot_price, Some(1.25));
        assert_eq!(log.calls.lock().unwrap().as_slice(), &[pk(0x01)]);
    }

    /// The hot path must not perform the fetch itself — it queues, and the
    /// dependency only lands once the resolver runs. Reverting to an inline
    /// await turns this red on the first assertion.
    #[tokio::test]
    async fn apply_queues_deps_and_the_resolver_fetches_them() {
        let dep_log = Arc::new(DispatchLog::default());
        let (ingest, mut resolver) = ingest_with_dep(
            HandleResult {
                accounts_to_fetch: vec![Dependency::new(pk(0xDE), DeliveryExpectation::Infrequent)],
                ..Default::default()
            },
            Arc::new(DispatchLog::default()),
            dep_log.clone(),
        );
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xDE), DISC_B, 101));

        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply");
        assert!(
            ingest.fetcher.calls().is_empty(),
            "apply must not fetch on the hot path"
        );

        resolver.drain().await;
        assert_eq!(dep_log.calls.lock().unwrap().as_slice(), &[pk(0xDE)]);
        assert_eq!(ingest.fetcher.calls(), vec![vec![pk(0xDE)]]);
    }

    /// A dependency resolves once, not once per update of the account that
    /// reports it — the dedup that keeps a per-update config reference from
    /// becoming a per-update RPC.
    #[tokio::test]
    async fn a_dependency_is_fetched_only_once() {
        let (ingest, mut resolver) = ingest_with_dep(
            HandleResult {
                accounts_to_fetch: vec![Dependency::new(pk(0xDE), DeliveryExpectation::Infrequent)],
                ..Default::default()
            },
            Arc::new(DispatchLog::default()),
            Arc::new(DispatchLog::default()),
        );
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xDE), DISC_B, 101));

        for slot in 0..5 {
            ingest
                .apply(update_for(pk(0xA0), pk(0x01), DISC_A, slot))
                .expect("apply");
        }
        resolver.drain().await;

        assert_eq!(ingest.fetcher.calls(), vec![vec![pk(0xDE)]]);
    }

    #[tokio::test]
    async fn resolver_resolves_transitive_deps() {
        // A → queues B → B's dispatch queues C. One drain resolves both.
        let log_b = Arc::new(DispatchLog::default());
        let log_c = Arc::new(DispatchLog::default());

        let mut registry: HandlerRegistry<()> = HandlerRegistry::new();
        registry.register(ConfiguredHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            response: HandleResult {
                accounts_to_fetch: vec![Dependency::new(pk(0xB1), DeliveryExpectation::Infrequent)],
                ..Default::default()
            },
            log: Arc::new(DispatchLog::default()),
        });
        registry.register(ConfiguredHandler {
            program_id: pk(0xB0),
            discriminator: DISC_B,
            response: HandleResult {
                accounts_to_fetch: vec![Dependency::new(pk(0xC1), DeliveryExpectation::Infrequent)],
                ..Default::default()
            },
            log: log_b.clone(),
        });
        registry.register(ConfiguredHandler {
            program_id: pk(0xB0),
            discriminator: DISC_C,
            response: HandleResult::default(),
            log: log_c.clone(),
        });

        let (ingest, mut resolver) = Ingest::new(registry, (), StubFetcher::new());
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xB1), DISC_B, 101));
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xC1), DISC_C, 102));

        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply");
        resolver.drain().await;

        assert_eq!(log_b.calls.lock().unwrap().as_slice(), &[pk(0xB1)]);
        assert_eq!(log_c.calls.lock().unwrap().as_slice(), &[pk(0xC1)]);
        assert_eq!(ingest.fetcher.calls(), vec![vec![pk(0xB1)], vec![pk(0xC1)]]);
    }

    #[tokio::test]
    async fn infrequent_subscribe_deps_are_bootstrapped_via_fetch() {
        // Reported under `accounts_to_subscribe` rather than `accounts_to_fetch`:
        // the class decides, not the list.
        let dep_log = Arc::new(DispatchLog::default());
        let (ingest, mut resolver) = ingest_with_dep(
            HandleResult {
                accounts_to_subscribe: vec![Dependency::new(
                    pk(0xDE),
                    DeliveryExpectation::Infrequent,
                )],
                ..Default::default()
            },
            Arc::new(DispatchLog::default()),
            dep_log.clone(),
        );
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xDE), DISC_B, 101));

        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply");
        resolver.drain().await;

        assert_eq!(dep_log.calls.lock().unwrap().as_slice(), &[pk(0xDE)]);
    }

    /// Neither `Frequent` (program-owned, already streaming) nor `Dynamic`
    /// (not program-owned, subscribed by pubkey) may be RPC-fetched. Fetching
    /// `Dynamic` is the case that collapsed ingestion ~20x once pool handlers
    /// began reporting their vaults. Reverting the class filter turns this red.
    #[tokio::test]
    async fn only_infrequent_deps_are_fetched() {
        let dep_log = Arc::new(DispatchLog::default());
        let (ingest, mut resolver) = ingest_with_dep(
            HandleResult {
                accounts_to_subscribe: vec![
                    Dependency::new(pk(0xDE), DeliveryExpectation::Dynamic),
                    Dependency::new(pk(0xDF), DeliveryExpectation::Frequent),
                ],
                ..Default::default()
            },
            Arc::new(DispatchLog::default()),
            dep_log.clone(),
        );
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xDE), DISC_B, 101));

        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply");
        resolver.drain().await;

        assert!(
            ingest.fetcher.calls().is_empty(),
            "only Infrequent deps may trigger an RPC fetch"
        );
        assert!(dep_log.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn accounts_pending_grpc_are_not_fetched() {
        // `accounts_pending_grpc` means "already coming via an existing
        // subscription" — ingest must NOT issue an RPC fetch for it.
        let log = Arc::new(DispatchLog::default());
        let (ingest, mut resolver) = make_ingest(vec![(
            pk(0xA0),
            DISC_A,
            HandleResult {
                accounts_pending_grpc: vec![Dependency::new(
                    pk(0xDE),
                    DeliveryExpectation::Frequent,
                )],
                ..Default::default()
            },
            log.clone(),
        )]);

        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply");
        resolver.drain().await;

        assert_eq!(log.calls.lock().unwrap().as_slice(), &[pk(0x01)]);
        assert!(ingest.fetcher.calls().is_empty());
    }

    #[tokio::test]
    async fn dep_without_handler_is_logged_but_does_not_abort_primary() {
        let primary_log = Arc::new(DispatchLog::default());
        let (ingest, mut resolver) = make_ingest(vec![(
            pk(0xA0),
            DISC_A,
            HandleResult {
                accounts_to_fetch: vec![Dependency::new(pk(0xDE), DeliveryExpectation::Infrequent)],
                ..Default::default()
            },
            primary_log.clone(),
        )]);

        // Fetcher returns an update, but its owner (pk 0xFF) has no handler.
        ingest.fetcher.add(AccountUpdate {
            pubkey: pk(0xDE),
            owner: pk(0xFF),
            data: disc_data(b"UNKNOWN_"),
            slot: 101,
        });

        let result = ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("primary still succeeds");
        resolver.drain().await;

        assert_eq!(result.accounts_to_fetch.len(), 1);
        assert_eq!(primary_log.calls.lock().unwrap().as_slice(), &[pk(0x01)]);
    }

    #[tokio::test]
    async fn primary_update_without_handler_errors_out() {
        let (ingest, _resolver) = make_ingest(vec![]);
        let err = ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .unwrap_err();
        matches!(
            err,
            IngestError::PrimaryHandler(HandlerError::NoHandler { .. })
        );
    }

    /// A failed fetch must not blacklist the dependency: the resolver un-sees
    /// it so the next update retries.
    #[tokio::test]
    async fn fetch_failure_is_retried_on_the_next_report() {
        let dep_log = Arc::new(DispatchLog::default());
        let (ingest, mut resolver) = ingest_with_dep(
            HandleResult {
                accounts_to_fetch: vec![Dependency::new(pk(0xDE), DeliveryExpectation::Infrequent)],
                ..Default::default()
            },
            Arc::new(DispatchLog::default()),
            dep_log.clone(),
        );
        ingest
            .fetcher
            .add(update_for(pk(0xB0), pk(0xDE), DISC_B, 101));

        ingest.fetcher.set_fail(true);
        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 100))
            .expect("apply survives a fetch failure");
        resolver.drain().await;
        assert!(dep_log.calls.lock().unwrap().is_empty());

        ingest.fetcher.set_fail(false);
        ingest
            .apply(update_for(pk(0xA0), pk(0x01), DISC_A, 101))
            .expect("apply");
        resolver.drain().await;
        assert_eq!(dep_log.calls.lock().unwrap().as_slice(), &[pk(0xDE)]);
    }
}
