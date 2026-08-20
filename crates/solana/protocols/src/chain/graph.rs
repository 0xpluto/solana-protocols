//! The accumulated set of known edges.
//!
//! Discovery is stateless per instruction; this is what remembers. It is
//! deliberately thin -- a map plus one rule -- because the interesting
//! question is not how to store an edge but what to do when two instructions
//! disagree about one.

use std::collections::HashMap;

use solana_program::pubkey::Pubkey;

use super::discovery::PoolEdge;
use crate::protocols::Protocol;

/// What observing an edge did to the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    /// A pool nobody had named before.
    New,
    /// Already known, and the pair matches. The common case.
    Agreed,
    /// Two instructions named **different pairs for the same pool**.
    ///
    /// One of them decoded the wrong account. This is the standing detector
    /// for that class: pumpfun's and pumpswap's `FeeConfig` PDAs already share
    /// an owner *and* a discriminator, so a pool decoder with the same shape
    /// would mis-attribute with nothing to show for it.
    ///
    /// The first edge is kept. Overwriting would let a decoder bug rewrite
    /// history quietly and leave the graph's final state depending on
    /// instruction order.
    Disagreed {
        /// What the graph already held, and still holds.
        kept: PoolEdge,
        /// What the new observation claimed.
        rejected: PoolEdge,
    },
}

/// Every pool we can name, keyed by the account that holds its reserves.
#[derive(Debug, Clone, Default)]
pub struct PoolGraph {
    by_pool: HashMap<Pubkey, PoolEdge>,
    disagreements: u64,
}

impl PoolGraph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an edge.
    pub fn observe(&mut self, edge: PoolEdge) -> Observed {
        match self.by_pool.get(&edge.pool) {
            None => {
                self.by_pool.insert(edge.pool, edge);
                Observed::New
            }
            Some(known) if known.same_pair(&edge) => Observed::Agreed,
            Some(known) => {
                self.disagreements += 1;
                Observed::Disagreed {
                    kept: *known,
                    rejected: edge,
                }
            }
        }
    }

    /// The edge for a pool, if we have named it.
    #[must_use]
    pub fn get(&self, pool: &Pubkey) -> Option<&PoolEdge> {
        self.by_pool.get(pool)
    }

    /// Every edge touching a token. Linear -- a token-keyed index is worth
    /// building when routing actually queries this, not before.
    pub fn touching<'a>(&'a self, token: &'a Pubkey) -> impl Iterator<Item = &'a PoolEdge> + 'a {
        self.by_pool
            .values()
            .filter(move |e| e.token_a == *token || e.token_b == *token)
    }

    /// How many pools we can name.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_pool.len()
    }

    /// Whether the graph is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_pool.is_empty()
    }

    /// How many times two observations disagreed about one pool's pair.
    ///
    /// Not a statistic: any value above zero means a decoder is reading the
    /// wrong account, and the graph is only as trustworthy as this is small.
    #[must_use]
    pub fn disagreements(&self) -> u64 {
        self.disagreements
    }

    /// Pools per protocol, for coverage reporting.
    #[must_use]
    pub fn by_protocol(&self) -> HashMap<Protocol, usize> {
        let mut out = HashMap::new();
        for e in self.by_pool.values() {
            *out.entry(e.protocol).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(pool: Pubkey, a: Pubkey, b: Pubkey) -> PoolEdge {
        PoolEdge::new(pool, Protocol::PumpSwap, a, b)
    }

    #[test]
    fn the_same_pool_traded_both_ways_is_one_edge() {
        let (pool, a, b) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut g = PoolGraph::new();
        assert_eq!(g.observe(edge(pool, a, b)), Observed::New);
        assert_eq!(g.observe(edge(pool, b, a)), Observed::Agreed);
        assert_eq!(g.len(), 1);
        assert_eq!(g.disagreements(), 0);
    }

    #[test]
    fn a_conflicting_pair_is_counted_and_the_first_one_kept() {
        let (pool, a, b, c) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut g = PoolGraph::new();
        g.observe(edge(pool, a, b));

        let Observed::Disagreed { kept, rejected } = g.observe(edge(pool, a, c)) else {
            panic!("a different pair for the same pool must not pass as agreement");
        };
        assert!(kept.same_pair(&edge(pool, a, b)));
        assert!(rejected.same_pair(&edge(pool, a, c)));
        assert_eq!(g.disagreements(), 1);
        assert!(
            g.get(&pool)
                .expect("still there")
                .same_pair(&edge(pool, a, b)),
            "the graph must not let a later observation rewrite an earlier one"
        );
    }

    #[test]
    fn touching_finds_both_sides() {
        let (a, b, c) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut g = PoolGraph::new();
        g.observe(edge(Pubkey::new_unique(), a, b));
        g.observe(edge(Pubkey::new_unique(), b, c));
        assert_eq!(g.touching(&b).count(), 2);
        assert_eq!(g.touching(&a).count(), 1);
        assert_eq!(g.touching(&Pubkey::new_unique()).count(), 0);
    }
}
