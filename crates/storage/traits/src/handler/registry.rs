//! Dispatch registry: keyed lookup + fallback scan.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use solana_pubkey::Pubkey;

use super::traits::{ProtocolStateHandler, StorageHandler};
use super::types::{HandleResult, HandlerError};

/// Object-safe facade over [`StorageHandler<C>`].
///
/// The typed trait has an associated `State` type and cannot be put behind
/// `dyn`. `ErasedHandler<C>` folds deserialize + apply into one call, with
/// the state type living entirely inside the handler. Every
/// `StorageHandler<C>` automatically implements this via a blanket impl.
pub trait ErasedHandler<C>: Send + Sync + 'static {
    fn program_id(&self) -> Pubkey;
    fn discriminator(&self) -> Option<&'static [u8]>;
    fn matches_account(&self, data: &[u8]) -> bool;
    fn subscribe_program_accounts(&self) -> bool;

    /// Deserialize and apply in one shot. The handler's state type never
    /// crosses this boundary.
    fn handle(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        data: &[u8],
        slot: u64,
    ) -> Result<HandleResult, HandlerError>;
}

impl<C, H> ErasedHandler<C> for H
where
    H: StorageHandler<C>,
    C: 'static,
{
    fn program_id(&self) -> Pubkey {
        <Self as ProtocolStateHandler>::program_id(self)
    }

    fn discriminator(&self) -> Option<&'static [u8]> {
        <Self as ProtocolStateHandler>::discriminator(self)
    }

    fn matches_account(&self, data: &[u8]) -> bool {
        <Self as ProtocolStateHandler>::matches_account(self, data)
    }

    fn subscribe_program_accounts(&self) -> bool {
        <Self as ProtocolStateHandler>::subscribe_program_accounts(self)
    }

    fn handle(
        &self,
        cache: &C,
        pubkey: &Pubkey,
        data: &[u8],
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        let state = <Self as ProtocolStateHandler>::deserialize(self, data)?;
        <Self as StorageHandler<C>>::apply(self, cache, pubkey, &state, slot)
    }
}

/// Primary dispatch key: program-owner paired with the first 8 bytes of the
/// account data (Anchor discriminator convention).
type DispatchKey = (Pubkey, [u8; 8]);

/// Stores registered handlers and dispatches account updates.
///
/// Two-level lookup:
///
/// 1. **Discriminator path (O(1)):** `(program_id, data[..8]) → handler`.
/// 2. **Fallback path (linear):** `program_id → [handler…]`, tried in
///    registration order; first match wins. A handler here is consulted only
///    if the account data starts with whatever discriminator it declared (a
///    handler declaring `None` skips that check), and then only if its
///    [`matches_account`](ErasedHandler::matches_account) returns `true`.
///
/// The fallback path is what carries **non-Anchor** programs: a single-byte
/// leading discriminator, a multi-byte prefix that is not 8 wide, or no
/// discriminator at all (where `matches_account` — size or sentinel checks —
/// is the entire test).
///
/// Generic over the cache type `C`. Daemons instantiate `HandlerRegistry<LocalCache>`.
pub struct HandlerRegistry<C: 'static> {
    by_discriminator: HashMap<DispatchKey, Arc<dyn ErasedHandler<C>>>,
    fallback: HashMap<Pubkey, Vec<Arc<dyn ErasedHandler<C>>>>,
    program_ids: HashSet<Pubkey>,
    /// Programs with at least one handler that wants their accounts
    /// wholesale (see [`ProtocolStateHandler::subscribe_program_accounts`]).
    subscribable: HashSet<Pubkey>,
}

impl<C: 'static> HandlerRegistry<C> {
    pub fn new() -> Self {
        Self {
            by_discriminator: HashMap::new(),
            fallback: HashMap::new(),
            program_ids: HashSet::new(),
            subscribable: HashSet::new(),
        }
    }

    /// Register a handler. Panics if another handler is already registered
    /// for the same `(program_id, discriminator)` tuple — conflicting
    /// discriminators are programmer errors, not runtime conditions.
    pub fn register<H>(&mut self, handler: H)
    where
        H: StorageHandler<C>,
    {
        let arc: Arc<dyn ErasedHandler<C>> = Arc::new(handler);
        let program_id = arc.program_id();
        self.program_ids.insert(program_id);
        // Subscribable iff *some* handler wants this program's accounts
        // wholesale; a dependency-only handler alone never widens the filter.
        if arc.subscribe_program_accounts() {
            self.subscribable.insert(program_id);
        }

        match arc.discriminator() {
            Some(d) if d.len() == 8 => {
                let mut key_bytes = [0u8; 8];
                key_bytes.copy_from_slice(d);
                let key: DispatchKey = (program_id, key_bytes);
                if self.by_discriminator.insert(key, arc).is_some() {
                    panic!(
                        "duplicate handler for program {program_id} + discriminator {key_bytes:?}"
                    );
                }
            }
            // None, or non-8-byte discriminator: fallback path.
            _ => {
                self.fallback.entry(program_id).or_default().push(arc);
            }
        }
    }

    /// Find the handler for an account update, if any.
    pub fn lookup(&self, owner: &Pubkey, data: &[u8]) -> Option<&dyn ErasedHandler<C>> {
        if let Some(first8) = first_eight(data) {
            let key: DispatchKey = (*owner, first8);
            if let Some(h) = self.by_discriminator.get(&key) {
                return Some(h.as_ref());
            }
        }
        if let Some(list) = self.fallback.get(owner) {
            for h in list {
                // Honour a declared discriminator even off the O(1) path.
                //
                // Without this, a handler declaring a short discriminator —
                // Raydium V5's single leading byte, say — was silently
                // downgraded to "match every account this program owns": the
                // bytes it declared were read once for their *length* in
                // `register`, used to route it here, and then never compared
                // against anything. Combined with `matches_account`'s
                // match-everything default, a precise declaration became a
                // catch-all with no diagnostic.
                if let Some(d) = h.discriminator() {
                    if !data.starts_with(d) {
                        continue;
                    }
                }
                if h.matches_account(data) {
                    return Some(h.as_ref());
                }
            }
        }
        None
    }

    /// Dispatch an account update. Equivalent to `lookup` + `handle`, with
    /// a `NoHandler` error if nothing matches.
    pub fn dispatch(
        &self,
        cache: &C,
        owner: &Pubkey,
        pubkey: &Pubkey,
        data: &[u8],
        slot: u64,
    ) -> Result<HandleResult, HandlerError> {
        match self.lookup(owner, data) {
            Some(handler) => handler.handle(cache, pubkey, data, slot),
            None => Err(HandlerError::NoHandler {
                program_id: *owner,
                discriminator: first_eight(data),
            }),
        }
    }

    /// Every program ID this registry has a handler for.
    ///
    /// **Not** the right list for a subscription filter — use
    /// [`subscribable_program_ids`](Self::subscribable_program_ids), which
    /// excludes dependency-only programs (e.g. SPL Token, whose accounts we
    /// subscribe to individually).
    pub fn program_ids(&self) -> impl Iterator<Item = &Pubkey> {
        self.program_ids.iter()
    }

    /// Program IDs whose accounts should be subscribed **wholesale** by owner.
    ///
    /// Use this to build gRPC subscription filters without maintaining a
    /// parallel list. Programs registered only by dependency-only handlers
    /// (`subscribe_program_accounts() == false`) are excluded: subscribing to
    /// every SPL-Token-owned account would be subscribing to most of Solana.
    pub fn subscribable_program_ids(&self) -> impl Iterator<Item = &Pubkey> {
        self.subscribable.iter()
    }

    pub fn len(&self) -> usize {
        self.by_discriminator.len() + self.fallback.values().map(Vec::len).sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.by_discriminator.is_empty() && self.fallback.is_empty()
    }
}

impl<C: 'static> Default for HandlerRegistry<C> {
    fn default() -> Self {
        Self::new()
    }
}

fn first_eight(data: &[u8]) -> Option<[u8; 8]> {
    data.get(..8)
        .and_then(|s| <&[u8; 8]>::try_from(s).ok())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{CacheInsert, InsertOutcome};
    use crate::handler::{DeliveryExpectation, Dependency};
    use dashmap::DashMap;
    use std::sync::Mutex;

    // --- Stub cache: satisfies CacheInsert<Pubkey, [u8; 32]>, no real
    // solana-account-cache dependency. Exercises the generic StorageHandler<C>
    // path with C = StubCache.

    #[derive(Default)]
    struct StubCache {
        entries: DashMap<Pubkey, ([u8; 32], u64)>,
    }

    impl CacheInsert<Pubkey, [u8; 32]> for StubCache {
        fn insert(&self, key: Pubkey, value: [u8; 32], slot: u64) -> InsertOutcome {
            self.entries.insert(key, (value, slot));
            InsertOutcome::Inserted
        }
    }

    // --- Test fixtures ----------------------------------------------------

    const DISC_A: &[u8; 8] = b"ALPHA\0\0\0";
    const DISC_B: &[u8; 8] = b"BETA\0\0\0\0";

    fn pk(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    #[derive(Default)]
    struct Invocations {
        applied: Mutex<Vec<(Pubkey, u64)>>,
    }

    /// Anchor-style handler: fixed program_id, 8-byte discriminator, decodes
    /// the [8..40] slice as a 32-byte value and stores it via CacheInsert.
    ///
    /// Crucially, this handler is generic over any cache `C` that implements
    /// `CacheInsert<Pubkey, [u8; 32]>` — it never references any concrete
    /// cache type. Same type works with `StubCache`, `LocalCache`, or any
    /// future cache implementation.
    struct AnchorHandler {
        program_id: Pubkey,
        discriminator: &'static [u8; 8],
        dep: Option<Dependency>,
        log: Arc<Invocations>,
    }

    impl ProtocolStateHandler for AnchorHandler {
        type State = [u8; 32];

        fn program_id(&self) -> Pubkey {
            self.program_id
        }
        fn discriminator(&self) -> Option<&'static [u8]> {
            Some(self.discriminator.as_slice())
        }
        fn deserialize(&self, data: &[u8]) -> Result<Self::State, HandlerError> {
            if data.len() < 8 + 32 {
                return Err(HandlerError::Deserialize {
                    data_len: data.len(),
                    reason: "need 40 bytes (8 disc + 32 state)".into(),
                });
            }
            let mut state = [0u8; 32];
            state.copy_from_slice(&data[8..40]);
            Ok(state)
        }
    }

    impl<C> StorageHandler<C> for AnchorHandler
    where
        C: CacheInsert<Pubkey, [u8; 32]> + Send + Sync + 'static,
    {
        fn apply(
            &self,
            cache: &C,
            pubkey: &Pubkey,
            state: &Self::State,
            slot: u64,
        ) -> Result<HandleResult, HandlerError> {
            cache.insert(*pubkey, *state, slot);
            self.log.applied.lock().unwrap().push((*pubkey, slot));
            let mut result = HandleResult::default();
            if let Some(d) = self.dep.clone() {
                result.accounts_to_fetch.push(d);
            }
            Ok(result)
        }
    }

    /// Fallback handler: no discriminator, matches on data length. Doesn't
    /// write to the cache — only exercises the dispatch path.
    struct SizeMatchHandler {
        program_id: Pubkey,
        expected_len: usize,
        log: Arc<Invocations>,
    }

    impl ProtocolStateHandler for SizeMatchHandler {
        type State = usize;
        fn program_id(&self) -> Pubkey {
            self.program_id
        }
        fn discriminator(&self) -> Option<&'static [u8]> {
            None
        }
        fn matches_account(&self, data: &[u8]) -> bool {
            data.len() == self.expected_len
        }
        fn deserialize(&self, data: &[u8]) -> Result<Self::State, HandlerError> {
            Ok(data.len())
        }
    }

    impl<C> StorageHandler<C> for SizeMatchHandler
    where
        C: Send + Sync + 'static,
    {
        fn apply(
            &self,
            _cache: &C,
            pubkey: &Pubkey,
            _state: &Self::State,
            slot: u64,
        ) -> Result<HandleResult, HandlerError> {
            self.log.applied.lock().unwrap().push((*pubkey, slot));
            Ok(HandleResult::default())
        }
    }

    fn make_anchor_update(disc: &[u8; 8], payload: [u8; 32]) -> Vec<u8> {
        let mut v = Vec::with_capacity(40);
        v.extend_from_slice(disc);
        v.extend_from_slice(&payload);
        v
    }

    /// Non-Anchor handler keyed on a **single** leading byte — the Raydium V5
    /// shape. Deliberately does *not* override `matches_account`, because the
    /// whole point is that the declared prefix must be enough on its own.
    struct BytePrefixHandler {
        program_id: Pubkey,
        prefix: &'static [u8],
        log: Arc<Invocations>,
    }

    impl ProtocolStateHandler for BytePrefixHandler {
        type State = usize;
        fn program_id(&self) -> Pubkey {
            self.program_id
        }
        fn discriminator(&self) -> Option<&'static [u8]> {
            Some(self.prefix)
        }
        fn deserialize(&self, data: &[u8]) -> Result<Self::State, HandlerError> {
            Ok(data.len())
        }
    }

    impl<C> StorageHandler<C> for BytePrefixHandler
    where
        C: Send + Sync + 'static,
    {
        fn apply(
            &self,
            _cache: &C,
            pubkey: &Pubkey,
            _state: &Self::State,
            slot: u64,
        ) -> Result<HandleResult, HandlerError> {
            self.log.applied.lock().unwrap().push((*pubkey, slot));
            Ok(HandleResult::default())
        }
    }

    // --- Tests -----------------------------------------------------------

    /// A short discriminator must be *checked*, not merely used to route the
    /// handler to the fallback list.
    ///
    /// Regression: `register` branched on `d.len() == 8` and everything else
    /// fell through to the fallback path, where `lookup` consulted only
    /// `matches_account` — whose default is `true`. So a handler declaring a
    /// single leading byte silently claimed every account the program owned,
    /// and the bytes it declared were never compared against anything.
    #[test]
    fn short_discriminator_is_enforced_not_ignored() {
        static PREFIX: &[u8] = &[0x05];
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(BytePrefixHandler {
            program_id: pk(0xB5),
            prefix: PREFIX,
            log: log.clone(),
        });
        let cache = StubCache::default();

        // Matching leading byte -> dispatches.
        reg.dispatch(&cache, &pk(0xB5), &pk(0x01), &[0x05, 0xAA, 0xBB], 10)
            .expect("prefix matches, must dispatch");

        // Same program, different leading byte -> must NOT dispatch. Before the
        // fix this returned the handler anyway.
        let err = reg
            .dispatch(&cache, &pk(0xB5), &pk(0x02), &[0x06, 0xAA, 0xBB], 11)
            .expect_err("non-matching prefix must not dispatch");
        assert!(matches!(err, HandlerError::NoHandler { .. }), "got {err:?}");

        let applied = log.applied.lock().unwrap();
        assert_eq!(
            applied.len(),
            1,
            "exactly the matching account, got {applied:?}"
        );
        assert_eq!(applied[0].0, pk(0x01));
    }

    /// Two non-Anchor handlers under one program must stay distinguishable.
    ///
    /// The fallback list is walked in registration order and the first match
    /// wins, so before the fix the first-registered handler shadowed every
    /// later one for that program — silently, with no duplicate-registration
    /// panic to catch it (unlike the 8-byte path).
    #[test]
    fn two_short_discriminators_under_one_program_do_not_shadow_each_other() {
        static A: &[u8] = &[0x01];
        static B: &[u8] = &[0x02];
        let log_a = Arc::new(Invocations::default());
        let log_b = Arc::new(Invocations::default());
        let prog = pk(0xC0);

        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(BytePrefixHandler {
            program_id: prog,
            prefix: A,
            log: log_a.clone(),
        });
        reg.register(BytePrefixHandler {
            program_id: prog,
            prefix: B,
            log: log_b.clone(),
        });
        let cache = StubCache::default();

        reg.dispatch(&cache, &prog, &pk(0x0A), &[0x01, 0xFF], 20)
            .expect("A");
        reg.dispatch(&cache, &prog, &pk(0x0B), &[0x02, 0xFF], 21)
            .expect("B");

        assert_eq!(
            log_a.applied.lock().unwrap().len(),
            1,
            "A took only its own"
        );
        assert_eq!(log_b.applied.lock().unwrap().len(), 1, "B was reachable");
    }

    /// A handler declaring no discriminator still reaches `matches_account` —
    /// the prefix check must not accidentally gate the predicate-only case.
    #[test]
    fn discriminator_less_handler_still_uses_its_predicate() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(SizeMatchHandler {
            program_id: pk(0xD0),
            expected_len: 3,
            log: log.clone(),
        });
        let cache = StubCache::default();

        reg.dispatch(&cache, &pk(0xD0), &pk(0x01), &[0u8; 3], 30)
            .expect("length matches");
        assert!(
            reg.dispatch(&cache, &pk(0xD0), &pk(0x02), &[0u8; 4], 31)
                .is_err(),
            "length differs, must not dispatch"
        );
        assert_eq!(log.applied.lock().unwrap().len(), 1);
    }

    #[test]
    fn discriminator_path_dispatches_to_the_right_handler() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });

        let cache = StubCache::default();
        let account = make_anchor_update(DISC_A, [0x11; 32]);
        reg.dispatch(&cache, &pk(0xA0), &pk(0x01), &account, 100)
            .expect("dispatch");

        assert_eq!(
            cache.entries.get(&pk(0x01)).map(|v| *v),
            Some(([0x11; 32], 100))
        );
        assert_eq!(log.applied.lock().unwrap().as_slice(), &[(pk(0x01), 100)]);
    }

    #[test]
    fn discriminators_are_scoped_per_program() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });
        reg.register(AnchorHandler {
            program_id: pk(0xB0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });

        let cache = StubCache::default();
        let account = make_anchor_update(DISC_A, [0x22; 32]);
        reg.dispatch(&cache, &pk(0xB0), &pk(0x02), &account, 200)
            .unwrap();

        assert_eq!(log.applied.lock().unwrap().as_slice(), &[(pk(0x02), 200)]);
    }

    #[test]
    fn unknown_owner_and_discriminator_returns_no_handler() {
        let reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        let cache = StubCache::default();
        let data = make_anchor_update(DISC_A, [0; 32]);
        match reg.dispatch(&cache, &pk(0xFF), &pk(0x01), &data, 1) {
            Err(HandlerError::NoHandler {
                program_id,
                discriminator,
            }) => {
                assert_eq!(program_id, pk(0xFF));
                assert_eq!(discriminator, Some(*DISC_A));
            }
            other => panic!("expected NoHandler, got {other:?}"),
        }
    }

    #[test]
    fn short_data_has_no_discriminator_and_still_reports_no_handler() {
        let reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        let cache = StubCache::default();
        match reg.dispatch(&cache, &pk(0xFF), &pk(0x01), &[1, 2, 3], 1) {
            Err(HandlerError::NoHandler {
                discriminator: None,
                ..
            }) => {}
            other => panic!("expected NoHandler{{discriminator:None}}, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_error_propagates_through_dispatch() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });
        let cache = StubCache::default();

        let truncated = DISC_A.to_vec();
        match reg.dispatch(&cache, &pk(0xA0), &pk(0x01), &truncated, 100) {
            Err(HandlerError::Deserialize { data_len: 8, .. }) => {}
            other => panic!("expected Deserialize error, got {other:?}"),
        }
        assert!(log.applied.lock().unwrap().is_empty());
    }

    #[test]
    fn fallback_handler_matches_on_size() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(SizeMatchHandler {
            program_id: pk(0xC0),
            expected_len: 165,
            log: log.clone(),
        });

        let cache = StubCache::default();
        reg.dispatch(&cache, &pk(0xC0), &pk(0x07), &[0u8; 165], 500)
            .expect("dispatch");
        assert_eq!(log.applied.lock().unwrap().as_slice(), &[(pk(0x07), 500)]);

        match reg.dispatch(&cache, &pk(0xC0), &pk(0x08), &[0u8; 200], 501) {
            Err(HandlerError::NoHandler { .. }) => {}
            other => panic!("expected NoHandler, got {other:?}"),
        }
    }

    #[test]
    fn fallback_handlers_tried_in_registration_order_first_match_wins() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(SizeMatchHandler {
            program_id: pk(0xC0),
            expected_len: 165,
            log: log.clone(),
        });

        struct SentinelHandler {
            program_id: Pubkey,
            sentinel: u8,
            log: Arc<Invocations>,
        }
        impl ProtocolStateHandler for SentinelHandler {
            type State = ();
            fn program_id(&self) -> Pubkey {
                self.program_id
            }
            fn discriminator(&self) -> Option<&'static [u8]> {
                None
            }
            fn matches_account(&self, data: &[u8]) -> bool {
                data.first() == Some(&self.sentinel)
            }
            fn deserialize(&self, _: &[u8]) -> Result<Self::State, HandlerError> {
                Ok(())
            }
        }
        impl<C: Send + Sync + 'static> StorageHandler<C> for SentinelHandler {
            fn apply(
                &self,
                _c: &C,
                pubkey: &Pubkey,
                _s: &Self::State,
                slot: u64,
            ) -> Result<HandleResult, HandlerError> {
                self.log.applied.lock().unwrap().push((*pubkey, slot));
                Ok(HandleResult::default())
            }
        }
        reg.register(SentinelHandler {
            program_id: pk(0xC0),
            sentinel: 0xEE,
            log: log.clone(),
        });

        let cache = StubCache::default();

        let mut data = vec![0u8; 165];
        data[0] = 0xEE;
        reg.dispatch(&cache, &pk(0xC0), &pk(0x09), &data, 600)
            .unwrap();
        assert_eq!(log.applied.lock().unwrap().as_slice(), &[(pk(0x09), 600)]);

        let mut data = vec![0u8; 10];
        data[0] = 0xEE;
        reg.dispatch(&cache, &pk(0xC0), &pk(0x0A), &data, 601)
            .unwrap();
        assert_eq!(
            log.applied.lock().unwrap().as_slice(),
            &[(pk(0x09), 600), (pk(0x0A), 601)]
        );
    }

    #[test]
    fn handle_result_dependencies_propagate_back_through_dispatch() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: Some(Dependency::new(pk(0xDE), DeliveryExpectation::Infrequent)),
            log: log.clone(),
        });

        let cache = StubCache::default();
        let account = make_anchor_update(DISC_A, [0; 32]);
        let result = reg
            .dispatch(&cache, &pk(0xA0), &pk(0x01), &account, 1)
            .unwrap();

        assert_eq!(result.accounts_to_fetch.len(), 1);
        assert_eq!(result.accounts_to_fetch[0].pubkey, pk(0xDE));
        assert_eq!(
            result.accounts_to_fetch[0].expectation,
            DeliveryExpectation::Infrequent,
        );
    }

    #[test]
    fn program_ids_enumerates_every_registered_owner() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_B,
            dep: None,
            log: log.clone(),
        });
        reg.register(SizeMatchHandler {
            program_id: pk(0xC0),
            expected_len: 165,
            log: log.clone(),
        });

        let ids: HashSet<Pubkey> = reg.program_ids().copied().collect();
        assert_eq!(ids, HashSet::from([pk(0xA0), pk(0xC0)]));
        assert_eq!(reg.len(), 3);
    }

    #[test]
    #[should_panic(expected = "duplicate handler")]
    fn duplicate_discriminator_registration_panics() {
        let log = Arc::new(Invocations::default());
        let mut reg: HandlerRegistry<StubCache> = HandlerRegistry::new();
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });
        reg.register(AnchorHandler {
            program_id: pk(0xA0),
            discriminator: DISC_A,
            dep: None,
            log: log.clone(),
        });
    }
}
