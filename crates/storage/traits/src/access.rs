//! Cache access contracts.
//!
//! Two traits — one for reads, one for writes — parameterized over `K` and
//! `V`. Callers bound their generics on these; the concrete cache type
//! stays behind the bound and is never imported directly.
//!
//! [`InsertOutcome`] is the shared return type for every slot-aware
//! insertion.

/// Outcome of a slot-aware insert into a cache field.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InsertOutcome {
    /// First observation for this key.
    Inserted,
    /// Same slot as the current entry — current overwritten in place, no
    /// history change.
    UpdatedSameSlot,
    /// Newer slot than current — previous current pushed to history (subject
    /// to the backend's depth), new value installed.
    Advanced,
    /// Older slot than current — rejected, backend unchanged.
    Stale,
}

/// Slot-aware read access to a `K → V` keyspace.
///
/// Implementors are expected to retain enough history for [`get_at_slot`] to
/// answer reference-slot queries within the configured retention window.
///
/// [`get_at_slot`]: CacheGet::get_at_slot
pub trait CacheGet<K, V> {
    /// Current value for `key`, if any.
    fn get(&self, key: &K) -> Option<V>;

    /// Current value for `key` paired with the slot it was recorded at.
    fn get_with_slot(&self, key: &K) -> Option<(V, u64)>;

    /// Most recent observation for `key` at or before `reference_slot`.
    fn get_at_slot(&self, key: &K, reference_slot: u64) -> Option<V>;

    /// Like [`get_at_slot`](Self::get_at_slot) but also returns the slot the
    /// observation was recorded at.
    fn get_at_slot_with_slot(&self, key: &K, reference_slot: u64) -> Option<(V, u64)>;
}

/// The cache's slot clock — the highest slot it has observed.
///
/// The read-side staleness primitive: a value read via
/// [`CacheGet::get_with_slot`] is `current_slot() - value_slot` slots old.
/// Consumers bound on this to *record* state age alongside every read;
/// gating on that age (a staleness ceiling) is policy and lives with the
/// consumer, never here.
pub trait CacheSlot {
    /// Highest slot the cache has observed. Zero until the first slot update.
    fn current_slot(&self) -> u64;
}

/// Slot-aware insert into a `K → V` keyspace.
pub trait CacheInsert<K, V> {
    /// Record `value` for `key` as observed at `slot`. Older-slot writes are
    /// rejected; same-slot writes overwrite; newer-slot writes advance.
    fn insert(&self, key: K, value: V, slot: u64) -> InsertOutcome;
}

/// Read/write access to a singleton value of type `T`.
///
/// Where [`CacheGet`]/[`CacheInsert`] model keyed fields ("the state of
/// account X at slot Y"), this trait models **unkeyed global values** —
/// the active fee recipient, a fee-config snapshot, a program's current
/// admin. One slot typically has one of these live at a time, so slot
/// history isn't meaningful; `get` returns an optional snapshot, `set`
/// replaces.
///
/// Implementors typically back this with `RwLock<Option<T>>`. Keep `T`
/// cheap to `Clone` — `get` takes a read lock briefly and hands out a
/// clone so downstream code never holds the lock across `.await`.
pub trait CacheSingleton<T> {
    /// Current value, if any. Returns `None` until the first `set`.
    fn get(&self) -> Option<T>;
    /// Replace the current value.
    fn set(&self, value: T);
}

/// Wire a slot-versioned field on `$cache` into both [`CacheGet`] and
/// [`CacheInsert`].
///
/// The accessor path can be a single field or a dotted sequence — both
/// forms resolve against `self`. The field must expose the usual slot-aware
/// read/write method surface (`get`, `get_with_slot`, `get_at_slot`,
/// `get_at_slot_with_slot`, `insert`).
///
/// ```ignore
/// use solana_account_traits::impl_cache_access;
///
/// struct LocalCache { data: Arc<CacheData> }
/// struct CacheData   { bonding_curves: VersionedStateMap<Pubkey, BondingCurve> }
/// impl_cache_access!(LocalCache, data.bonding_curves, Pubkey, BondingCurve);
/// ```
#[macro_export]
macro_rules! impl_cache_access {
    ($cache:ty, $($seg:ident).+, $key:ty, $value:ty) => {
        impl $crate::CacheGet<$key, $value> for $cache {
            fn get(&self, key: &$key) -> ::core::option::Option<$value> {
                self.$($seg).+.get(key)
            }

            fn get_with_slot(
                &self,
                key: &$key,
            ) -> ::core::option::Option<($value, u64)> {
                self.$($seg).+.get_with_slot(key)
            }

            fn get_at_slot(
                &self,
                key: &$key,
                reference_slot: u64,
            ) -> ::core::option::Option<$value> {
                self.$($seg).+.get_at_slot(key, reference_slot)
            }

            fn get_at_slot_with_slot(
                &self,
                key: &$key,
                reference_slot: u64,
            ) -> ::core::option::Option<($value, u64)> {
                self.$($seg).+.get_at_slot_with_slot(key, reference_slot)
            }
        }

        impl $crate::CacheInsert<$key, $value> for $cache {
            fn insert(
                &self,
                key: $key,
                value: $value,
                slot: u64,
            ) -> $crate::InsertOutcome {
                self.$($seg).+.insert(key, value, slot)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashmap::DashMap;
    use std::hash::Hash;

    // --- Stub slot-versioned map with just enough surface for the macro to
    // compile against — exercises the access contracts without pulling in
    // solana-account-cache's VersionedStateMap.

    struct StubVersionedMap<K: Eq + Hash + Clone, V: Clone> {
        current: DashMap<K, (V, u64)>,
    }

    impl<K: Eq + Hash + Clone, V: Clone> StubVersionedMap<K, V> {
        fn new() -> Self {
            Self {
                current: DashMap::new(),
            }
        }

        fn get(&self, key: &K) -> Option<V> {
            self.current.get(key).map(|v| v.0.clone())
        }

        fn get_with_slot(&self, key: &K) -> Option<(V, u64)> {
            self.current.get(key).map(|v| v.clone())
        }

        fn get_at_slot(&self, key: &K, reference_slot: u64) -> Option<V> {
            self.current
                .get(key)
                .filter(|v| v.1 <= reference_slot)
                .map(|v| v.0.clone())
        }

        fn get_at_slot_with_slot(&self, key: &K, reference_slot: u64) -> Option<(V, u64)> {
            self.current
                .get(key)
                .filter(|v| v.1 <= reference_slot)
                .map(|v| v.clone())
        }

        fn insert(&self, key: K, value: V, slot: u64) -> InsertOutcome {
            use dashmap::mapref::entry::Entry;
            match self.current.entry(key) {
                Entry::Vacant(v) => {
                    v.insert((value, slot));
                    InsertOutcome::Inserted
                }
                Entry::Occupied(mut o) => {
                    let (_, existing_slot) = *o.get();
                    if slot < existing_slot {
                        InsertOutcome::Stale
                    } else if slot == existing_slot {
                        o.insert((value, slot));
                        InsertOutcome::UpdatedSameSlot
                    } else {
                        o.insert((value, slot));
                        InsertOutcome::Advanced
                    }
                }
            }
        }
    }

    struct TestCache {
        by_id: StubVersionedMap<u32, String>,
        by_pair: StubVersionedMap<(u32, u32), u64>,
    }

    impl TestCache {
        fn new() -> Self {
            Self {
                by_id: StubVersionedMap::new(),
                by_pair: StubVersionedMap::new(),
            }
        }
    }

    impl_cache_access!(TestCache, by_id, u32, String);
    impl_cache_access!(TestCache, by_pair, (u32, u32), u64);

    #[test]
    fn macro_wires_both_traits_for_a_plain_field() {
        let cache = TestCache::new();
        <TestCache as CacheInsert<u32, String>>::insert(&cache, 1, "a".into(), 100);
        <TestCache as CacheInsert<u32, String>>::insert(&cache, 1, "b".into(), 101);
        assert_eq!(
            <TestCache as CacheGet<u32, String>>::get(&cache, &1).as_deref(),
            Some("b")
        );
        // get_at_slot reaches the stub's implementation through the trait.
        // (The stub has no history, so slot=100 returns None now that
        // "b"@101 is current — that's the stub's behavior, not the trait's.)
        assert_eq!(
            <TestCache as CacheGet<u32, String>>::get_at_slot(&cache, &1, 101).as_deref(),
            Some("b"),
        );
        assert_eq!(
            <TestCache as CacheGet<u32, String>>::get_at_slot(&cache, &1, 100),
            None,
        );
    }

    #[test]
    fn insert_outcome_variants_propagate_through_the_trait() {
        let cache = TestCache::new();
        assert_eq!(
            <TestCache as CacheInsert<u32, String>>::insert(&cache, 1, "a".into(), 100),
            InsertOutcome::Inserted,
        );
        assert_eq!(
            <TestCache as CacheInsert<u32, String>>::insert(&cache, 1, "a2".into(), 100),
            InsertOutcome::UpdatedSameSlot,
        );
        assert_eq!(
            <TestCache as CacheInsert<u32, String>>::insert(&cache, 1, "b".into(), 101),
            InsertOutcome::Advanced,
        );
        assert_eq!(
            <TestCache as CacheInsert<u32, String>>::insert(&cache, 1, "stale".into(), 100),
            InsertOutcome::Stale,
        );
    }

    #[test]
    fn composite_key_field_participates_in_the_same_traits() {
        let cache = TestCache::new();
        <TestCache as CacheInsert<(u32, u32), u64>>::insert(&cache, (10, 20), 999, 100);
        assert_eq!(
            <TestCache as CacheGet<(u32, u32), u64>>::get(&cache, &(10, 20)),
            Some(999),
        );
    }

    #[test]
    fn cache_get_is_object_safe() {
        let cache = TestCache::new();
        <TestCache as CacheInsert<u32, String>>::insert(&cache, 7, "x".into(), 500);
        let trait_ref: &dyn CacheGet<u32, String> = &cache;
        assert_eq!(trait_ref.get_with_slot(&7), Some(("x".to_string(), 500)));
    }

    // Hand-rolled impl — proves the macro isn't the only path: type
    // translation across the trait boundary works too.
    struct HandRolled {
        inner: StubVersionedMap<u32, i64>,
    }

    impl CacheGet<u32, String> for HandRolled {
        fn get(&self, key: &u32) -> Option<String> {
            self.inner.get(key).map(|v| format!("#{v}"))
        }
        fn get_with_slot(&self, key: &u32) -> Option<(String, u64)> {
            self.inner
                .get_with_slot(key)
                .map(|(v, s)| (format!("#{v}"), s))
        }
        fn get_at_slot(&self, key: &u32, slot: u64) -> Option<String> {
            self.inner.get_at_slot(key, slot).map(|v| format!("#{v}"))
        }
        fn get_at_slot_with_slot(&self, key: &u32, slot: u64) -> Option<(String, u64)> {
            self.inner
                .get_at_slot_with_slot(key, slot)
                .map(|(v, s)| (format!("#{v}"), s))
        }
    }

    #[test]
    fn hand_written_impl_can_translate_types_across_the_trait_boundary() {
        let cache = HandRolled {
            inner: StubVersionedMap::new(),
        };
        cache.inner.insert(42, 7i64, 100);
        assert_eq!(
            <HandRolled as CacheGet<u32, String>>::get(&cache, &42).as_deref(),
            Some("#7"),
        );
    }
}
