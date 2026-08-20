//! Pool discovery: the four facts a route-discovery graph needs per pool.
//!
//! An edge is `(pool, protocol, token_a, token_b)`. All four sit in a swap
//! instruction's **accounts**, so discovery reads the account list and the
//! 8-byte discriminator and nothing else. It deliberately does not decode the
//! instruction body, walk the log stream, or wait for an event:
//!
//! * event bodies and log slicing are the two layers with the worst silent
//!   failures on record here, and an edge that depends on them disappears
//!   quietly when they break;
//! * an extractor needs the event to report amounts, so it yields no
//!   [`ChainEvent`](super::ChainEvent) at all when the event is missing --
//!   even though the pool and both mints were sitting in the accounts.
//!
//! Discovery therefore covers strictly more than extraction, and depends on
//! strictly less: a discriminator and an accounts struct, both IDL-checked and
//! fixture-pinned.

use solana_program::pubkey::Pubkey;

use crate::pairs::SwapAccounts;
use crate::parsing::ParsedInstruction;
use crate::protocols::Protocol;
use crate::swap_instruction::{self, SwapInstruction};

/// One tradeable edge: which pool, whose math, and the two tokens it joins.
///
/// `token_a` and `token_b` are sorted by their bytes at construction, so the
/// same pool yields the same edge whichever way it was traded. Direction is a
/// property of a trade, not of the edge, and a pair that flipped with
/// direction would make every buy disagree with every sell of the same pool.
///
/// Which side is the quote is deliberately not kept: `raydium_cpmm` names its
/// mints `input`/`output`, so base-vs-quote is not recoverable there without
/// reading the pool account, and a field that is only sometimes meaningful is
/// worse than one that is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolEdge {
    /// The account holding reserves -- AMM pool, bonding curve, or CLMM pool.
    pub pool: Pubkey,
    /// Whose math and account layout apply.
    pub protocol: Protocol,
    /// One side of the pair. Ordered before [`token_b`](Self::token_b).
    pub token_a: Pubkey,
    /// The other side.
    pub token_b: Pubkey,
}

impl PoolEdge {
    /// Build an edge, ordering the pair canonically.
    #[must_use]
    pub fn new(pool: Pubkey, protocol: Protocol, one: Pubkey, other: Pubkey) -> Self {
        let (token_a, token_b) = if one.to_bytes() <= other.to_bytes() {
            (one, other)
        } else {
            (other, one)
        };
        Self {
            pool,
            protocol,
            token_a,
            token_b,
        }
    }

    /// Whether two edges name the same pair. Ordering is canonical, so this is
    /// a field comparison -- it exists to say what the comparison *means*.
    #[must_use]
    pub fn same_pair(&self, other: &Self) -> bool {
        self.token_a == other.token_a && self.token_b == other.token_b
    }
}

/// What one instruction had to say about a pool.
///
/// The variants are kept apart because each is a different piece of work.
/// Collapsing them into `Option<PoolEdge>` would merge "nothing to see here"
/// with "a swap we cannot read", and the second is a todo nobody would find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// Both tokens named. The edge is complete.
    Edge(PoolEdge),
    /// A catalogued swap whose accounts this crate cannot read yet, so not
    /// even the pool is recoverable. Carries the instruction so the gap can be
    /// counted per instruction rather than guessed at.
    Unreadable(SwapInstruction),
    /// Not a swap on a protocol we know -- an admin call, a router's own
    /// instruction, an uncatalogued discriminator. Routine.
    NotASwap,
}

/// Read whatever a single instruction can tell us about a pool.
///
/// Never fetches, never fails: an instruction it cannot read comes back as
/// [`Unreadable`](Discovery::Unreadable), never as [`NotASwap`].
#[must_use]
pub fn discover(ix: &ParsedInstruction) -> Discovery {
    use crate::protocols::{pumpfun, pumpswap};

    let instruction = swap_instruction::resolve(&ix.program_id, &ix.data);
    let keys = ix.accounts.as_slice();

    // Every arm is spelled out: `SwapInstruction` gains a variant whenever a
    // protocol gains a swap, and a wildcard here would let that new swap
    // silently stop producing edges.
    //
    // Which accounts hold the pool and the mints is not decided here -- it
    // lives in each layout's `SwapAccounts` impl, beside the fields.
    match instruction {
        SwapInstruction::PumpfunBuy => {
            read::<pumpfun::BuyAccounts>(instruction, keys, Protocol::Pumpfun)
        }
        SwapInstruction::PumpfunSell => {
            read::<pumpfun::SellAccounts>(instruction, keys, Protocol::Pumpfun)
        }
        SwapInstruction::PumpfunBuyV2 => {
            read::<pumpfun::BuyV2Accounts>(instruction, keys, Protocol::Pumpfun)
        }
        SwapInstruction::PumpfunSellV2 => {
            read::<pumpfun::SellV2Accounts>(instruction, keys, Protocol::Pumpfun)
        }
        SwapInstruction::PumpSwapBuy => {
            read::<pumpswap::BuyAccounts>(instruction, keys, Protocol::PumpSwap)
        }
        SwapInstruction::PumpSwapSell => {
            read::<pumpswap::SellAccounts>(instruction, keys, Protocol::PumpSwap)
        }
        // No accounts struct of their own. Each is documented as sharing a
        // sibling's layout, but a positional read of pubkeys succeeds whatever
        // the order is, so borrowing the sibling's struct would produce
        // confident edges built from unverified slots. Deriving the pool from
        // the mints settles it -- until then these are a named gap, not a
        // guess. `buy_exact_quote_in` even has a 30-account shape its sibling
        // has never been seen in.
        SwapInstruction::PumpfunBuyExactSolIn
        | SwapInstruction::PumpfunBuyExactQuoteInV2
        | SwapInstruction::PumpSwapBuyExactQuoteIn => Discovery::Unreadable(instruction),
        SwapInstruction::Unknown(_) => Discovery::NotASwap,
    }
}

/// Pull the four facts out of an account list via the layout's own impl.
fn read<A: SwapAccounts>(
    instruction: SwapInstruction,
    keys: &[Pubkey],
    protocol: Protocol,
) -> Discovery {
    match A::read(keys) {
        Some((pool, (a, b))) => Discovery::Edge(PoolEdge::new(pool, protocol, a, b)),
        // The accounts struct refuses a list it cannot name, which is the
        // point -- a short or reordered list must not yield a confident edge
        // built from the wrong slots. But this *is* a catalogued swap, so the
        // refusal is a defect to count, never a quiet "nothing here".
        None => Discovery::Unreadable(instruction),
    }
}
