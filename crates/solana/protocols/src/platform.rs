//! Which trading platform a transaction was submitted through.
//!
//! Trading UIs (and aggregators) route orders through their own program, so
//! the **top-level program of a swap transaction names the tool the trader
//! used**. That is a direct observation of someone's tooling rather than an
//! inference from behaviour, which makes it the sharpest wallet-fingerprint
//! signal available — and unlike timing or sizing statistics, it cannot be
//! imitated.
//!
//! # Why [`Other`] carries the program id
//!
//! Sampled 2026-08-11 over 107 decoded swap transactions: **13 distinct
//! router programs**, most appearing once or twice, against 98 unrouted
//! (direct) calls. The router population is an **open set with a long tail**,
//! so a closed enum would silently discard every platform we have not yet
//! named. The id is ground truth and is always recorded; the *name* is a
//! curated label that can improve without re-recording a single row.
//!
//! Naming a variant is therefore a deliberate, evidence-backed act. Only
//! [`Jupiter`] is named today because its program id is the one that can be
//! identified beyond doubt. Vanity prefixes (`FLASHX…`, `Prism…`, `T1TAN…`)
//! are suggestive, not proof, and this repo has already paid for treating a
//! hand-transcribed program id as fact (`FEE_PROGRAM_ID`, which was never a
//! program at all).
//!
//! [`Other`]: TradingPlatform::Other
//! [`Jupiter`]: TradingPlatform::Jupiter

use solana_program::pubkey::Pubkey;

use crate::parsing::ParsedInstruction;
use crate::protocols::Protocol;

/// Jupiter aggregator v6.
pub const JUPITER_V6: Pubkey =
    solana_program::pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

/// Axiom Trade — operator-confirmed 2026-08-11.
pub const AXIOM: Pubkey = solana_program::pubkey!("FLASHX8DrLbgeR8FcfNV1F5krxYcYMUdBkrP1EPBtxB9");

/// DFlow aggregator — operator-confirmed 2026-08-11.
pub const DFLOW: Pubkey = solana_program::pubkey!("DF1ow4tspfHX9JwWJsAb9epbkA8hmpSEAtxXy1V27QBH");

/// Cielo's router — operator-confirmed 2026-08-11.
pub const CIELO: Pubkey = solana_program::pubkey!("7F2k5rXvmthLXJWCzxoCFbcuHdJx3JL1FxHFKSVxs8QB");

/// pump.fun's own router programs. **Evidence** (2026-08-11): both carry
/// upgrade authority `7gZufwwAo17y5kg8FMyJy2phgpvv9RSdzWtdXiWHjFr8`, which is
/// byte-identical to the authority of the confirmed `pumpfun` AND `pumpswap`
/// programs. Attributed to the OPERATOR, not to a product name — two distinct
/// programs, deliberately not collapsed into a brand.
pub const PUMPFUN_ROUTER_A: Pubkey =
    solana_program::pubkey!("6Vo3245eszAb5wuqEMw8mGdbfRUdKbHhDHP5LcaGuTAB");
pub const PUMPFUN_ROUTER_B: Pubkey =
    solana_program::pubkey!("MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e");

/// GMGN. **Evidence**: both program ids are vanity-`GMgn`/`GMGN`, AND both
/// share upgrade authority `dgmgqBJi4DczjSWpv5QeFco8Q8JG3uiNsxXahKbAU4F`,
/// itself vanity-`dgmgq`. Program vanity alone is weak; the authority
/// agreeing independently is what raises this to probable.
pub const GMGN_A: Pubkey = solana_program::pubkey!("GMgnVFR8Jb39LoXsEVzb3DvBy3ywCmdmJquHUy1Lrkqb");
pub const GMGN_B: Pubkey = solana_program::pubkey!("GMGNreQcJFufBiCTLDBgKhYEfEe9B454UjpDr5CaSLA1");

/// Maestro. **Evidence**: upgrade authority `MaestroUL88UBnZr3wfoN7hqmNWFi…`
/// carries the same vanity as the program id, independently.
pub const MAESTRO: Pubkey = solana_program::pubkey!("MaestroAAe9ge5HTc64VbBQZ6fP77pwvrhM8i1XWSAx");

/// Bloom. **Evidence**: upgrade authority `b1oomXDWMeH1CUXeDqcFNRziEg49…`
/// carries the same vanity as the program id, independently.
pub const BLOOM: Pubkey = solana_program::pubkey!("b1oomGGqPKGD6errbyfbVMBuzSC8WtAAYo8MwNafWW1");

/// Named platforms and their program ids: the ONE table both directions
/// derive from, so `from_program_id` and [`program`](TradingPlatform::program)
/// cannot disagree. Restating a mechanical mapping as a second hand-written
/// list is precisely the bug that left `Protocol::from_program_id` missing a
/// variant and misfiling 4,096 swaps an hour.
const NAMED: &[(Pubkey, TradingPlatform)] = &[
    (JUPITER_V6, TradingPlatform::Jupiter),
    (AXIOM, TradingPlatform::Axiom),
    (DFLOW, TradingPlatform::DFlow),
    (CIELO, TradingPlatform::Cielo),
    // Many ids MAY map to one platform: operators run several programs (and
    // shard them). `program()` returns the first as canonical; the exact id
    // is on every tape row regardless.
    (PUMPFUN_ROUTER_A, TradingPlatform::PumpFunRouter),
    (PUMPFUN_ROUTER_B, TradingPlatform::PumpFunRouter),
    (GMGN_A, TradingPlatform::Gmgn),
    (GMGN_B, TradingPlatform::Gmgn),
    (MAESTRO, TradingPlatform::Maestro),
    (BLOOM, TradingPlatform::Bloom),
];

/// The compute-budget program: fee/limit setup that rides along with almost
/// every trade and is never the platform.
pub const COMPUTE_BUDGET: Pubkey =
    solana_program::pubkey!("ComputeBudget111111111111111111111111111111");

/// SPL Memo (v1 and v2). Routers and bots attach memos to their trades, so
/// it shows up at top level and looks like a platform — 1,179 swaps were
/// misfiled to it before this was filtered (observed 2026-08-11).
pub const MEMO_V2: Pubkey = solana_program::pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const MEMO_V1: Pubkey = solana_program::pubkey!("Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo");

/// The platform a swap transaction was submitted through.
///
/// See the module docs for why the unidentified case carries its program id
/// rather than collapsing to a bare "unknown".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TradingPlatform {
    /// No router: the swap program itself was the top-level instruction.
    /// Custom infrastructure — bots, arb, direct protocol integration — and
    /// measured as the majority of raw swap count, which is the tell that
    /// most swaps are machines rather than people at a UI.
    Direct,
    /// Jupiter aggregator v6.
    Jupiter,
    /// Axiom Trade.
    Axiom,
    /// DFlow aggregator.
    DFlow,
    /// Cielo's router.
    Cielo,
    /// pump.fun's own router (two programs, one operator).
    PumpFunRouter,
    /// GMGN (two programs, one operator).
    Gmgn,
    /// Maestro.
    Maestro,
    /// Bloom.
    Bloom,
    /// A router observed but not yet identified by product.
    Other(Pubkey),
    /// No top-level program survived the companion filter — the transaction
    /// carried nothing but setup instructions. Distinct from [`Direct`]:
    /// absence of evidence, not evidence of a direct call.
    ///
    /// [`Direct`]: Self::Direct
    Unattributed,
}

impl TradingPlatform {
    /// Stable lowercase name for grouping. Unidentified routers all render
    /// `"other"`; use [`program`](Self::program) to tell them apart.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Jupiter => "jupiter",
            Self::Axiom => "axiom",
            Self::DFlow => "dflow",
            Self::Cielo => "cielo",
            Self::PumpFunRouter => "pumpfun_router",
            Self::Gmgn => "gmgn",
            Self::Maestro => "maestro",
            Self::Bloom => "bloom",
            Self::Other(_) => "other",
            Self::Unattributed => "unattributed",
        }
    }

    /// The router's program id, or `None` when nothing was routed through.
    #[must_use]
    pub fn program(self) -> Option<Pubkey> {
        match self {
            Self::Other(p) => Some(p),
            Self::Direct | Self::Unattributed => None,
            named => NAMED.iter().find(|(_, p)| *p == named).map(|(id, _)| *id),
        }
    }

    /// Classify a single top-level program id.
    #[must_use]
    pub fn from_program_id(program: &Pubkey) -> Self {
        if let Some((_, named)) = NAMED.iter().find(|(id, _)| id == program) {
            *named
        } else if Protocol::from_program_id(program).is_some() {
            Self::Direct
        } else {
            Self::Other(*program)
        }
    }
}

/// Programs that ride along with a trade without being the platform: compute
/// budget, account creation, plain token transfers, lamport moves.
fn is_companion(program: &Pubkey) -> bool {
    *program == COMPUTE_BUDGET
        || *program == MEMO_V1
        || *program == MEMO_V2
        || *program == solana_program::system_program::id()
        || *program == spl_associated_token_account::id()
        || *program == spl_token::id()
        || *program == spl_token_2022::id()
}

/// Resolve the platform from a transaction's flattened instruction list.
///
/// The platform is the first top-level (`stack_height == 1`) instruction that
/// is not a companion program. Multi-hop routes keep one platform for the
/// whole transaction, which is correct: routing is a property of how the
/// transaction was submitted, not of an individual swap leg.
#[must_use]
pub fn resolve(instructions: &[ParsedInstruction]) -> TradingPlatform {
    instructions
        .iter()
        .filter(|ix| ix.stack_height == 1 && !is_companion(&ix.program_id))
        .map(|ix| TradingPlatform::from_program_id(&ix.program_id))
        .next()
        .unwrap_or(TradingPlatform::Unattributed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::ParsedInstructionBuilder;

    fn ix(program: Pubkey, stack_height: u32) -> ParsedInstruction {
        ParsedInstructionBuilder::new()
            .program_id(program)
            .accounts(vec![])
            .data(vec![])
            .instruction_index(0)
            .stack_height(stack_height)
            .build()
    }

    const PUMPFUN: Pubkey = solana_program::pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");

    #[test]
    fn a_router_at_top_level_is_the_platform() {
        let router = Pubkey::new_unique();
        // Real shape: compute-budget setup, then the router, which CPIs down
        // into the AMM (stack_height 2) — the AMM must NOT win.
        let p = resolve(&[ix(COMPUTE_BUDGET, 1), ix(router, 1), ix(PUMPFUN, 2)]);
        assert_eq!(p, TradingPlatform::Other(router));
        assert_eq!(p.name(), "other");
        assert_eq!(p.program(), Some(router), "the id is never lost");
    }

    #[test]
    fn an_amm_at_top_level_is_a_direct_call() {
        let p = resolve(&[ix(COMPUTE_BUDGET, 1), ix(PUMPFUN, 1)]);
        assert_eq!(p, TradingPlatform::Direct);
        assert_eq!(p.program(), None, "nothing was routed through");
    }

    #[test]
    fn jupiter_is_named_and_every_companion_is_skipped() {
        assert_eq!(resolve(&[ix(JUPITER_V6, 1)]), TradingPlatform::Jupiter);
        for companion in [
            COMPUTE_BUDGET,
            solana_program::system_program::id(),
            spl_associated_token_account::id(),
            spl_token::id(),
            spl_token_2022::id(),
        ] {
            assert!(
                is_companion(&companion),
                "{companion} must not be a platform"
            );
        }
    }

    /// Every named variant must be reachable from its program id and resolve
    /// back to it. The exhaustive match means adding a variant fails to
    /// compile until it is listed, so the enum and `NAMED` cannot drift —
    /// the failure mode that left `Protocol::from_program_id` a variant short.
    #[test]
    fn every_named_platform_round_trips_through_the_table() {
        fn requires_table_entry(p: TradingPlatform) -> bool {
            match p {
                TradingPlatform::Jupiter
                | TradingPlatform::Axiom
                | TradingPlatform::DFlow
                | TradingPlatform::Cielo
                | TradingPlatform::PumpFunRouter
                | TradingPlatform::Gmgn
                | TradingPlatform::Maestro
                | TradingPlatform::Bloom => true,
                TradingPlatform::Direct
                | TradingPlatform::Other(_)
                | TradingPlatform::Unattributed => false,
            }
        }
        for named in [
            TradingPlatform::Jupiter,
            TradingPlatform::Axiom,
            TradingPlatform::DFlow,
            TradingPlatform::Cielo,
            TradingPlatform::PumpFunRouter,
            TradingPlatform::Gmgn,
            TradingPlatform::Maestro,
            TradingPlatform::Bloom,
        ] {
            assert!(requires_table_entry(named));
            let id = named.program().expect("named platform has an id");
            assert_eq!(TradingPlatform::from_program_id(&id), named);
        }
        // The operator-confirmed ids, pinned so a typo cannot pass review.
        assert_eq!(TradingPlatform::from_program_id(&AXIOM).name(), "axiom");
        assert_eq!(TradingPlatform::from_program_id(&DFLOW).name(), "dflow");
        assert_eq!(TradingPlatform::from_program_id(&CIELO).name(), "cielo");
        // Many-to-one: every id of a multi-program operator resolves to the
        // same platform, which is what makes a sharded deployment countable.
        for id in [PUMPFUN_ROUTER_A, PUMPFUN_ROUTER_B] {
            assert_eq!(
                TradingPlatform::from_program_id(&id),
                TradingPlatform::PumpFunRouter
            );
        }
        for id in [GMGN_A, GMGN_B] {
            assert_eq!(TradingPlatform::from_program_id(&id), TradingPlatform::Gmgn);
        }
    }

    #[test]
    fn memo_is_a_companion_not_a_platform() {
        // A router attaching a memo must not be recorded AS the memo program.
        let router = Pubkey::new_unique();
        assert_eq!(
            resolve(&[ix(MEMO_V2, 1), ix(router, 1), ix(PUMPFUN, 2)]),
            TradingPlatform::Other(router)
        );
        assert!(is_companion(&MEMO_V1) && is_companion(&MEMO_V2));
    }

    #[test]
    fn companions_only_is_unattributed_not_direct() {
        // Absence of evidence must not read as "direct call".
        let p = resolve(&[ix(COMPUTE_BUDGET, 1), ix(spl_token::id(), 1)]);
        assert_eq!(p, TradingPlatform::Unattributed);
        assert_eq!(p.program(), None);
    }
}
