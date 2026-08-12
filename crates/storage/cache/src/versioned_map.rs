//! Slot-versioned concurrent map.
//!
//! `VersionedStateMap<K, T>` is the primitive the cache is built on. Every
//! insertion carries the slot the state was observed at, stale writes are
//! rejected, and a bounded history per key lets readers reconstruct state as
//! of a chosen reference slot (needed for cross-account consistency when
//! different accounts update at different rates).

use std::collections::VecDeque;
use std::hash::Hash;

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use solana_account_traits::InsertOutcome;

/// A value paired with the slot it was observed at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateWithSlot<T> {
    pub state: T,
    pub slot: u64,
}

impl<T> StateWithSlot<T> {
    pub fn new(state: T, slot: u64) -> Self {
        Self { state, slot }
    }
}

/// The current observation for a key plus a bounded history of prior ones,
/// newest-first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedState<T> {
    pub current: StateWithSlot<T>,
    pub history: VecDeque<StateWithSlot<T>>,
}

impl<T> VersionedState<T> {
    fn new(state: T, slot: u64) -> Self {
        Self {
            current: StateWithSlot::new(state, slot),
            history: VecDeque::new(),
        }
    }
}

/// Concurrent slot-versioned map from `K` to `T`.
///
/// Backed by [`DashMap`]; cheap concurrent reads and writes. Each entry
/// retains up to `depth` historical observations in addition to the current
/// one.
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionedStateMap<K, T>
where
    K: Eq + Hash,
{
    inner: DashMap<K, VersionedState<T>>,
    depth: usize,
}

impl<K, T> VersionedStateMap<K, T>
where
    K: Eq + Hash + Clone,
    T: Clone,
{
    /// Create a new map retaining `depth` historical observations per key.
    ///
    /// `depth = 0` discards history immediately on every slot advance (only
    /// the current observation is kept).
    pub fn with_depth(depth: usize) -> Self {
        Self {
            inner: DashMap::new(),
            depth,
        }
    }

    /// Number of keys in the map.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Configured history depth.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Insert an observation at `slot`.
    pub fn insert(&self, key: K, state: T, slot: u64) -> InsertOutcome {
        match self.inner.entry(key) {
            Entry::Vacant(v) => {
                v.insert(VersionedState::new(state, slot));
                InsertOutcome::Inserted
            }
            Entry::Occupied(mut o) => {
                let versioned = o.get_mut();
                if slot < versioned.current.slot {
                    InsertOutcome::Stale
                } else if slot == versioned.current.slot {
                    versioned.current.state = state;
                    InsertOutcome::UpdatedSameSlot
                } else {
                    let prev =
                        std::mem::replace(&mut versioned.current, StateWithSlot::new(state, slot));
                    if self.depth > 0 {
                        versioned.history.push_front(prev);
                        while versioned.history.len() > self.depth {
                            versioned.history.pop_back();
                        }
                    }
                    InsertOutcome::Advanced
                }
            }
        }
    }

    /// Get the current value for `key`.
    pub fn get(&self, key: &K) -> Option<T> {
        self.inner.get(key).map(|v| v.current.state.clone())
    }

    /// Get the current value for `key`, paired with its slot.
    pub fn get_with_slot(&self, key: &K) -> Option<(T, u64)> {
        self.inner
            .get(key)
            .map(|v| (v.current.state.clone(), v.current.slot))
    }

    /// Get the most recent observation at or before `reference_slot`.
    ///
    /// Walks newest-first: current first, then history front-to-back. Returns
    /// `None` if every retained observation for `key` is newer than
    /// `reference_slot` (or the key is absent).
    pub fn get_at_slot(&self, key: &K, reference_slot: u64) -> Option<T> {
        self.get_at_slot_with_slot(key, reference_slot)
            .map(|(s, _)| s)
    }

    /// Like [`get_at_slot`](Self::get_at_slot) but also returns the slot the
    /// observation was recorded at.
    pub fn get_at_slot_with_slot(&self, key: &K, reference_slot: u64) -> Option<(T, u64)> {
        self.inner.get(key).and_then(|v| {
            if v.current.slot <= reference_slot {
                return Some((v.current.state.clone(), v.current.slot));
            }
            v.history
                .iter()
                .find(|s| s.slot <= reference_slot)
                .map(|s| (s.state.clone(), s.slot))
        })
    }

    /// Remove a key, returning its current value if present.
    pub fn remove(&self, key: &K) -> Option<T> {
        self.inner.remove(key).map(|(_, v)| v.current.state)
    }

    pub fn clear(&self) {
        self.inner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_insert_reports_inserted_and_is_readable() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(4);
        assert_eq!(map.insert(1, "a", 100), InsertOutcome::Inserted);
        assert_eq!(map.get(&1), Some("a"));
        assert_eq!(map.get_with_slot(&1), Some(("a", 100)));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn same_slot_insert_overwrites_current_without_touching_history() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(4);
        map.insert(1, "a", 100);
        assert_eq!(map.insert(1, "b", 100), InsertOutcome::UpdatedSameSlot);
        assert_eq!(map.get(&1), Some("b"));
        // No history created by a same-slot update.
        assert_eq!(map.get_at_slot(&1, 99), None);
    }

    #[test]
    fn newer_slot_insert_advances_and_promotes_previous_to_history() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(4);
        map.insert(1, "a", 100);
        assert_eq!(map.insert(1, "b", 101), InsertOutcome::Advanced);
        assert_eq!(map.get(&1), Some("b"));
        assert_eq!(map.get_at_slot(&1, 100), Some("a"));
        assert_eq!(map.get_at_slot(&1, 101), Some("b"));
    }

    #[test]
    fn older_slot_insert_is_rejected_as_stale() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(4);
        map.insert(1, "a", 100);
        assert_eq!(map.insert(1, "old", 99), InsertOutcome::Stale);
        assert_eq!(map.get(&1), Some("a"));
    }

    #[test]
    fn get_at_slot_returns_none_when_reference_precedes_all_observations() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(4);
        map.insert(1, "a", 100);
        assert_eq!(map.get_at_slot(&1, 99), None);
    }

    #[test]
    fn get_at_slot_walks_newest_first_and_returns_nearest_le_observation() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(8);
        map.insert(1, "a", 100);
        map.insert(1, "b", 105);
        map.insert(1, "c", 110);
        map.insert(1, "d", 120);

        // exact matches
        assert_eq!(map.get_at_slot(&1, 100), Some("a"));
        assert_eq!(map.get_at_slot(&1, 105), Some("b"));
        assert_eq!(map.get_at_slot(&1, 110), Some("c"));
        assert_eq!(map.get_at_slot(&1, 120), Some("d"));

        // between-slot queries return the nearest <= observation
        assert_eq!(map.get_at_slot(&1, 104), Some("a"));
        assert_eq!(map.get_at_slot(&1, 119), Some("c"));
        assert_eq!(map.get_at_slot(&1, 999), Some("d"));
    }

    #[test]
    fn depth_zero_keeps_no_history_across_slot_advances() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(0);
        map.insert(1, "a", 100);
        map.insert(1, "b", 101);
        assert_eq!(map.get(&1), Some("b"));
        // Prior observation was discarded.
        assert_eq!(map.get_at_slot(&1, 100), None);
        // Reference slot >= current still resolves to current.
        assert_eq!(map.get_at_slot(&1, 101), Some("b"));
    }

    #[test]
    fn history_is_bounded_by_depth_and_drops_oldest() {
        let map: VersionedStateMap<u32, u32> = VersionedStateMap::with_depth(2);
        for (slot, value) in [(100, 0u32), (101, 1), (102, 2), (103, 3), (104, 4)] {
            map.insert(1, value, slot);
        }
        // current = 4@104, history holds at most 2: {3@103, 2@102}
        assert_eq!(map.get(&1), Some(4));
        assert_eq!(map.get_at_slot(&1, 103), Some(3));
        assert_eq!(map.get_at_slot(&1, 102), Some(2));
        // Older entries evicted.
        assert_eq!(map.get_at_slot(&1, 101), None);
        assert_eq!(map.get_at_slot(&1, 100), None);
    }

    #[test]
    fn remove_returns_current_and_clear_empties_the_map() {
        let map: VersionedStateMap<u32, &str> = VersionedStateMap::with_depth(2);
        map.insert(1, "a", 100);
        map.insert(1, "b", 101);
        map.insert(2, "x", 100);
        assert_eq!(map.remove(&1), Some("b"));
        assert_eq!(map.get(&1), None);
        assert_eq!(map.len(), 1);
        map.clear();
        assert!(map.is_empty());
    }

    #[test]
    fn concurrent_writers_do_not_lose_updates_and_end_in_consistent_state() {
        use std::sync::Arc;
        use std::thread;

        let map: Arc<VersionedStateMap<u32, u64>> = Arc::new(VersionedStateMap::with_depth(16));
        let threads = 8;
        let inserts_per_thread = 200;

        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let map = Arc::clone(&map);
                thread::spawn(move || {
                    for i in 0..inserts_per_thread {
                        // Each thread writes to its own key; slots are monotonic per key.
                        map.insert(t as u32, i as u64, i as u64);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(map.len(), threads);
        for t in 0..threads {
            // Final observation for each key is the last slot written.
            let last = (inserts_per_thread - 1) as u64;
            assert_eq!(map.get_with_slot(&(t as u32)), Some((last, last)));
        }
    }

    #[test]
    fn serialize_roundtrip_preserves_current_and_history() {
        let map: VersionedStateMap<u32, String> = VersionedStateMap::with_depth(2);
        map.insert(1, "a".to_string(), 100);
        map.insert(1, "b".to_string(), 101);
        map.insert(1, "c".to_string(), 102);

        let json = serde_json::to_string(&map).expect("serialize");
        let restored: VersionedStateMap<u32, String> =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.depth(), 2);
        assert_eq!(restored.get(&1), Some("c".to_string()));
        assert_eq!(restored.get_at_slot(&1, 101), Some("b".to_string()));
        assert_eq!(restored.get_at_slot(&1, 100), Some("a".to_string()));
    }
}
