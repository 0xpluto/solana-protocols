//! Per-protocol extractor trait + program_id → fn registry.
//!
//! Each protocol submodule implements [`ProtocolExtractor`] as a
//! zero-sized adapter — usually 30-50 lines of "decode my instruction,
//! build the right ChainEvent variant." The registry dispatches by
//! program ID so [`extract_transaction`](super::extract_transaction)
//! doesn't grow a linear match chain as we add protocols.
//!
//! # Adding a protocol
//!
//! 1. Create `extract/<protocol>.rs` with a `struct FooExtractor;`
//!    implementing [`ProtocolExtractor`].
//! 2. Add `mod <protocol>;` to `extract/mod.rs`.
//! 3. Add `registry.register::<FooExtractor>()` to
//!    [`ExtractorRegistry::with_all_protocols`] — one line.

use std::collections::HashMap;

use solana_program::pubkey::Pubkey;

use super::context::ExtractContext;
use crate::chain::types::ChainEvent;
use crate::parsing::ParsedInstruction;

/// Opaque function type stored in the registry. Takes the current
/// instruction, the full flat instruction list (for walking parent /
/// sibling relationships), and an [`ExtractContext`] trait object.
pub type ExtractFn =
    fn(&ParsedInstruction, &[ParsedInstruction], &dyn ExtractContext) -> Option<ChainEvent>;

/// Implemented by each protocol's extractor adapter.
///
/// The adapter holds no state — it's a handle on a static dispatch
/// function. Consts, not methods, carry the per-protocol identifiers
/// so the registry can register by type without constructing an
/// instance.
///
/// # Why `all_instructions`?
///
/// Anchor `emit_cpi!`-style events are emitted as inner self-CPI
/// instructions, not program-data logs. Their payload lives on the
/// inner ix and they need to walk [`ParsedInstruction::parent_index`]
/// back to the outer swap ix for account context (mint addresses,
/// trader identity). Passing the full slice keeps the API uniform:
/// legacy `emit!` protocols (Pumpfun) just ignore it.
pub trait ProtocolExtractor: 'static {
    /// Program ID this extractor handles.
    fn program_id() -> Pubkey;

    /// Turn a parsed instruction into a semantic [`ChainEvent`], or
    /// return `None` when:
    ///
    /// * The instruction discriminator isn't one we care about (admin
    ///   ops, bonding-curve-internal ops, sibling CPIs of a swap, etc.)
    /// * Required log/transfer data is missing (truncated logs, failed
    ///   txs slipping through).
    /// * Parsing the instruction payload failed — the impl should
    ///   `warn!` and return `None` rather than panic.
    ///
    /// The extractor is dispatched on **every** instruction whose
    /// `program_id` matches [`program_id()`](Self::program_id) — outer
    /// swap ixs and any inner self-CPI event ixs alike. The impl is
    /// responsible for deciding which variant to emit an event on (and
    /// returning `None` for the others so each swap produces exactly
    /// one [`ChainEvent`]).
    fn extract(
        ix: &ParsedInstruction,
        all_instructions: &[ParsedInstruction],
        ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent>;
}

/// Program_id → extractor function lookup.
pub struct ExtractorRegistry {
    by_program: HashMap<Pubkey, ExtractFn>,
}

impl ExtractorRegistry {
    /// Empty registry — no protocols wired in.
    pub fn new() -> Self {
        Self {
            by_program: HashMap::new(),
        }
    }

    /// Registry with every protocol the crate knows about. Per-protocol
    /// extractors live in `protocols/<name>/extract.rs`; one line per
    /// protocol here.
    pub fn with_all_protocols() -> Self {
        let mut r = Self::new();
        r.register::<crate::protocols::pumpfun::PumpfunExtractor>();
        r.register::<crate::protocols::pumpswap::PumpSwapExtractor>();
        r.register::<crate::protocols::meteora_damm_v2::MeteoraDammV2Extractor>();
        r.register::<crate::protocols::meteora_dlmm::MeteoraDlmmExtractor>();
        r
    }

    /// Register one protocol's extractor. Panics on duplicate
    /// registrations — same program_id twice is a programmer error.
    pub fn register<E: ProtocolExtractor>(&mut self) {
        let program_id = E::program_id();
        if self.by_program.insert(program_id, E::extract).is_some() {
            panic!("duplicate extractor registration for program {program_id}");
        }
    }

    /// Look up the extractor for a given program ID.
    pub fn get(&self, program_id: &Pubkey) -> Option<ExtractFn> {
        self.by_program.get(program_id).copied()
    }

    /// Run the registered extractor (if any) against an instruction.
    pub fn extract(
        &self,
        ix: &ParsedInstruction,
        all_instructions: &[ParsedInstruction],
        ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        let f = self.get(&ix.program_id)?;
        f(ix, all_instructions, ctx)
    }

    /// All registered program IDs. Useful for building gRPC tx
    /// subscription filters without keeping a parallel list.
    pub fn program_ids(&self) -> impl Iterator<Item = &Pubkey> {
        self.by_program.keys()
    }

    pub fn len(&self) -> usize {
        self.by_program.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_program.is_empty()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::with_all_protocols()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::extract::NoContext;
    use crate::chain::types::{ChainEvent, Swap};
    use crate::parsing::ParsedInstructionBuilder;
    use crate::protocols::Protocol;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    struct FakeExtractor;

    impl ProtocolExtractor for FakeExtractor {
        fn program_id() -> Pubkey {
            pk(0xAA)
        }

        fn extract(
            _ix: &ParsedInstruction,
            _all_instructions: &[ParsedInstruction],
            _ctx: &dyn ExtractContext,
        ) -> Option<ChainEvent> {
            Some(ChainEvent::Swap(Swap {
                completed_curve: false,
                track_volume: crate::protocols::OptionBool::None,
                instruction: crate::swap_instruction::SwapInstruction::Unknown([0; 8]),
                protocol: Protocol::Pumpfun, // tag doesn't matter for this test
                pool: pk(0x01),
                trader: pk(0x02),
                token_in: pk(0x03),
                amount_in: 1,
                token_out: pk(0x04),
                amount_out: 1,
                fee_amount: 0,
                fee_mint: pk(0x03),
                state_before: None,
                state_after: None,
            }))
        }
    }

    #[test]
    fn registry_dispatches_by_program_id() {
        let mut r = ExtractorRegistry::new();
        r.register::<FakeExtractor>();

        let hit = ParsedInstructionBuilder::new()
            .program_id(FakeExtractor::program_id())
            .data(vec![0; 8])
            .build();
        assert!(r
            .extract(&hit, std::slice::from_ref(&hit), &NoContext)
            .is_some());

        let miss = ParsedInstructionBuilder::new()
            .program_id(pk(0xBB))
            .data(vec![0; 8])
            .build();
        assert!(r
            .extract(&miss, std::slice::from_ref(&miss), &NoContext)
            .is_none());
    }

    #[test]
    #[should_panic(expected = "duplicate extractor registration")]
    fn duplicate_registration_panics() {
        let mut r = ExtractorRegistry::new();
        r.register::<FakeExtractor>();
        r.register::<FakeExtractor>();
    }

    #[test]
    fn program_ids_enumerates_registered_protocols() {
        let r = ExtractorRegistry::with_all_protocols();
        let ids: Vec<_> = r.program_ids().copied().collect();
        assert!(ids.contains(&crate::protocols::pumpfun::PROGRAM_ID));
    }
}
