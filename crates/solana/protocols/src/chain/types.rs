//! Consumer-facing semantic events from parsed Solana transactions.
//!
//! This is the **output shape** of the transaction parser — the distilled
//! stream every downstream consumer reads. Contrast with
//! [`crate::parsing`], which is the machinery for getting here:
//!
//! ```text
//! raw tx → parsing::ParsedInstruction + TransactionContext
//!        → extractor (per protocol)
//!        → chain::ParsedTransaction  ← (this module)
//! ```
//!
//! # Design choices
//!
//! * **Tx-envelope, not a flat event stream.** A single tx can produce
//!   multiple events (Jupiter multi-hop, "creator buys own mint" rug
//!   patterns, migration + initial buy). Preserving the grouping costs
//!   nothing — consumers that want flat just `flat_map`.
//! * **Executed amounts, not declared.** Every field in [`Swap`] is what
//!   actually moved — resolved from CPI transfers or on-chain event
//!   logs. Instruction params (slippage bounds) are discarded at the
//!   extractor layer.
//! * **Two primary atoms** (`Swap`, `TokenCreation`) plus
//!   `Migration` for the bonding-curve-to-AMM transition. No liquidity
//!   add/remove, no pool-creation — no consumer needs them today.
//! * **[`TxError::Slippage`] first-class.** Reactive-trader branches on
//!   it. Everything else collapses to `Rejected(program_id, code)` —
//!   full typed error enums per protocol are a later optimisation.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signature;

use crate::parsing::ParsedInstruction;
use crate::protocols::Protocol;

// =============================================================================
// Top-level wrapper
// =============================================================================

/// A fully-parsed Solana transaction — one per confirmed signature.
///
/// Produced by the transaction extractor; consumed by market-data,
/// trade-tracker, reactive-trader, and the pool-discovery worker. The
/// parser emits one of these per observed tx, regardless of whether it
/// contained anything we cared about ([`events`](Self::events) is empty
/// for txs with no [`ChainEvent`] variants, and for failed txs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTransaction {
    pub signature: Signature,
    pub slot: u64,
    /// Position of this transaction within its slot. The finest-grained
    /// ordering the stream carries — who won a contested launch — and a
    /// **stream-only** fact: unlike `block_time` it cannot be recovered by a
    /// later RPC backfill, so it is captured here at parse time.
    pub index: u64,
    /// Unix seconds. `None` when the source didn't carry it (gRPC often).
    pub block_time: Option<i64>,
    /// Fee paid in lamports (regardless of success).
    pub fee_paid_lamports: u64,
    /// Compute units consumed. `None` if the source didn't report it.
    pub compute_used: Option<u64>,
    pub outcome: TxOutcome,
    /// Empty when [`outcome`](Self::outcome) is `Failed`, or when the tx
    /// didn't touch any protocol we extract.
    pub events: Vec<ChainEvent>,
    /// Flat instruction list (top-level + inner, parent-linked via
    /// [`ParsedInstruction::parent_index`]). Kept here so downstream
    /// consumers can run structural scans over the tx — e.g.
    /// graduation detection looking for a PumpSwap `CreatePool` ix —
    /// without the extractor throwing this data away.
    ///
    /// Consumers that only care about semantic [`events`](Self::events)
    /// can ignore this field. Empty when the extractor ran on a failed
    /// tx (we don't persist state transitions that never occurred).
    pub instructions: Vec<ParsedInstruction>,
    /// Pre→post token-account balances from the tx meta — every token
    /// account this tx touched, paired by account index. Holdings ground
    /// truth at this tx's instant: unlike a fold over swap events it sees
    /// the effect of plain transfers too. Empty for failed txs (no state
    /// changed) and for sources whose meta didn't carry balances —
    /// `#[serde(default)]` keeps older serialized forms readable.
    #[serde(default)]
    pub token_balances: Vec<TokenBalanceChange>,
}

impl ParsedTransaction {
    /// Iterate just the [`Swap`] events, ignoring other variants.
    pub fn swaps(&self) -> impl Iterator<Item = &Swap> {
        self.events.iter().filter_map(|e| match e {
            ChainEvent::Swap(s) => Some(s),
            ChainEvent::TokenCreation(_) | ChainEvent::Migration(_) => None,
        })
    }

    /// Iterate just the [`TokenCreation`] events.
    pub fn token_creations(&self) -> impl Iterator<Item = &TokenCreation> {
        self.events.iter().filter_map(|e| match e {
            ChainEvent::TokenCreation(c) => Some(c),
            ChainEvent::Swap(_) | ChainEvent::Migration(_) => None,
        })
    }

    /// Iterate just the [`Migration`] events.
    pub fn migrations(&self) -> impl Iterator<Item = &Migration> {
        self.events.iter().filter_map(|e| match e {
            ChainEvent::Migration(m) => Some(m),
            ChainEvent::Swap(_) | ChainEvent::TokenCreation(_) => None,
        })
    }

    /// True if the tx was confirmed successful on-chain.
    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, TxOutcome::Success)
    }
}

// =============================================================================
// Token balances
// =============================================================================

/// One side's reported balance for one token account in a tx's meta —
/// the raw material [`TokenBalanceChange::pair`] pairs up.
///
/// Proto-free by design: whichever adapter bridges the raw tx source
/// (Yellowstone gRPC, RPC) maps wire entries into this shape at the same
/// boundary where it builds the
/// [`TransactionHeader`](crate::chain::TransactionHeader). Amounts are
/// parsed from the wire's decimal *string*, never through `f64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenBalanceEntry {
    /// Index into the tx's combined account-key list. Pairing key only —
    /// deliberately not resolved to the token-account address (resolution
    /// needs the LUT-loaded keys; `owner` + `mint` already identify the
    /// holding).
    pub account_index: u32,
    /// Token program owning the account (SPL Token vs Token-2022).
    pub program: Pubkey,
    /// Wallet that owns the token account.
    pub owner: Pubkey,
    pub mint: Pubkey,
    /// Balance in raw base units.
    pub raw: u64,
    pub decimals: u8,
}

/// Pre→post balance of one token account across one transaction.
///
/// `None` means the meta reported no entry on that side: a missing pre is
/// an account this tx created (it held nothing before); a missing post is
/// an account this tx closed. `None` is typed absence, never zero — a
/// reported zero balance stays `Some(0)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBalanceChange {
    /// Wallet that owns the token account.
    pub owner: Pubkey,
    pub mint: Pubkey,
    /// Token program owning the account (SPL Token vs Token-2022).
    pub program: Pubkey,
    pub pre_raw: Option<u64>,
    pub post_raw: Option<u64>,
    pub decimals: u8,
}

impl TokenBalanceChange {
    /// Pair pre/post entries by `account_index`.
    ///
    /// An index present on both sides but with a different `(owner, mint)`
    /// is *not* one account's balance change — the account at that index
    /// was closed and re-created within the tx — so it yields two unpaired
    /// changes rather than a lying delta. Output order: post entries in
    /// arrival order, then unmatched pre entries in arrival order.
    #[must_use]
    pub fn pair(pre: Vec<TokenBalanceEntry>, post: Vec<TokenBalanceEntry>) -> Vec<Self> {
        let mut consumed = vec![false; pre.len()];
        let mut out = Vec::with_capacity(pre.len().max(post.len()));
        for p in &post {
            // Tiny n (a handful of token accounts per tx) — linear scan
            // beats a map here and keeps ordering deterministic.
            let matched = pre
                .iter()
                .enumerate()
                .find(|(i, q)| !consumed[*i] && q.account_index == p.account_index);
            match matched {
                Some((i, q)) if q.owner == p.owner && q.mint == p.mint => {
                    consumed[i] = true;
                    out.push(Self {
                        owner: p.owner,
                        mint: p.mint,
                        program: p.program,
                        pre_raw: Some(q.raw),
                        post_raw: Some(p.raw),
                        decimals: p.decimals,
                    });
                }
                Some((i, q)) => {
                    // Same index, different account identity: emit both
                    // sides unpaired.
                    consumed[i] = true;
                    out.push(Self {
                        owner: q.owner,
                        mint: q.mint,
                        program: q.program,
                        pre_raw: Some(q.raw),
                        post_raw: None,
                        decimals: q.decimals,
                    });
                    out.push(Self {
                        owner: p.owner,
                        mint: p.mint,
                        program: p.program,
                        pre_raw: None,
                        post_raw: Some(p.raw),
                        decimals: p.decimals,
                    });
                }
                None => out.push(Self {
                    owner: p.owner,
                    mint: p.mint,
                    program: p.program,
                    pre_raw: None,
                    post_raw: Some(p.raw),
                    decimals: p.decimals,
                }),
            }
        }
        for (i, q) in pre.into_iter().enumerate() {
            if !consumed[i] {
                out.push(Self {
                    owner: q.owner,
                    mint: q.mint,
                    program: q.program,
                    pre_raw: Some(q.raw),
                    post_raw: None,
                    decimals: q.decimals,
                });
            }
        }
        out
    }

    /// Net raw-unit change, treating a missing side as zero holdings.
    /// Positive = the owner's balance grew.
    #[must_use]
    pub fn delta_raw(&self) -> i128 {
        i128::from(self.post_raw.unwrap_or(0)) - i128::from(self.pre_raw.unwrap_or(0))
    }
}

/// Success / failure outcome of the transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TxOutcome {
    Success,
    Failed(TxError),
}

/// Reason a transaction failed on-chain.
///
/// Intentionally coarse — `Slippage` is the only variant consumers
/// branch on today. Typed per-protocol error enums can be added behind
/// `Rejected` when a consumer needs them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TxError {
    /// Any protocol's slippage-exceeded error. Reactive-trader uses
    /// this to decide "retry wider" vs "give up".
    Slippage,
    /// Insufficient SOL / token balance to perform the tx.
    InsufficientFunds,
    /// Anchor constraint violation, unknown program error, etc. Carries
    /// the program that returned the error + its raw custom code so a
    /// consumer can decode it protocol-specifically if needed.
    Rejected {
        program_id: Pubkey,
        custom_code: u32,
    },
    /// Couldn't attribute the error — logs truncated, unknown code,
    /// non-custom `TransactionError`. Keeps the raw message for
    /// debugging.
    Other(String),
}

// =============================================================================
// Semantic event variants
// =============================================================================

/// A single semantic event extracted from a transaction.
///
/// `#[non_exhaustive]` — new variants may be added. Downstream match
/// sites are expected to include a fallback (or update deliberately).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ChainEvent {
    Swap(Swap),
    TokenCreation(TokenCreation),
    Migration(Migration),
}

impl ChainEvent {
    pub fn protocol(&self) -> Protocol {
        match self {
            ChainEvent::Swap(s) => s.protocol,
            ChainEvent::TokenCreation(c) => c.protocol,
            ChainEvent::Migration(m) => m.from_protocol,
        }
    }
}

/// A token swap — the dominant event shape. Drives price updates,
/// volume aggregation, trade tracking, and quoting-math calibration.
///
/// # Token-agnostic
///
/// The pair is expressed purely as `(token_in, token_out)`. We don't
/// tag either side as "base" or "quote" — that's a consumer-side
/// convention (a SOL/USDC trading app will treat WSOL as base; a
/// USDC/USDT stablecoin dashboard will pick differently). Direction
/// and price are derivable from these fields plus the consumer's base
/// mint and mint decimals.
///
/// # Amount conventions
///
/// * [`amount_in`](Self::amount_in) is **gross** — the full amount that
///   entered the pool, pre fee. `fee_amount` / `fee_mint` give the fee
///   separately so callers can reconstruct net values.
/// * [`amount_out`](Self::amount_out) is what the user actually received.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Swap {
    pub protocol: Protocol,
    /// Which instruction produced this swap.
    ///
    /// The unit the quote math is constant over — **not** buy-vs-sell.
    /// Pumpfun ships six swap instructions and PumpSwap three, and they
    /// disagree about which amount the user pinned, so grading them together
    /// averages several different formulas into one. See
    /// [`SwapInstruction`](crate::swap_instruction::SwapInstruction).
    pub instruction: crate::swap_instruction::SwapInstruction,
    /// Whether the instruction asked the program to track volume, as it
    /// appeared on the wire.
    ///
    /// A pump-family argument, so [`OptionBool::None`] on every other
    /// protocol — and there it means "this protocol has no such argument",
    /// which is why the column is nullable downstream rather than defaulted
    /// to false.
    ///
    /// Recorded because it is not cosmetic: the flag adds an account to the
    /// instruction (28 vs 27 on `buy_exact_quote_in_v2`, measured over 1,050
    /// live instructions), so it should move compute, and that is only
    /// checkable if the flag rides the same row as `compute_used`.
    ///
    /// [`OptionBool::None`]: crate::protocols::OptionBool::None
    pub track_volume: crate::protocols::OptionBool,
    /// Bonding curve, AMM pool, or CLMM pool — whichever primitive
    /// holds reserves for this protocol.
    pub pool: Pubkey,
    /// Address that signed the tx / paid the fee.
    pub trader: Pubkey,

    /// Mint the trader paid in.
    pub token_in: Pubkey,
    /// Gross amount of `token_in` that entered the pool.
    pub amount_in: u64,

    /// Mint the trader received.
    pub token_out: Pubkey,
    /// Amount of `token_out` delivered to the trader.
    pub amount_out: u64,

    /// Total fee charged (protocol + creator + LP + any partner /
    /// referral splits the protocol reports). `0` when the extractor
    /// doesn't yet decode fees for this protocol.
    pub fee_amount: u64,
    /// Mint `fee_amount` is denominated in. Usually [`token_in`]
    /// (input-side fee) or [`token_out`] (output-side fee). When fees
    /// aren't decoded, defaults to [`token_in`].
    ///
    /// [`token_in`]: Self::token_in
    /// [`token_out`]: Self::token_out
    pub fee_mint: Pubkey,

    /// Curve state immediately **before** this swap, when the protocol's event
    /// log reports it.
    ///
    /// Protocols disagree about which side they publish, and the disagreement
    /// is not guessable — it was settled by replaying consecutive swaps per
    /// pool off our own tape (2026-08-10, ~9k pairs):
    ///
    /// | protocol | publishes | evidence |
    /// |---|---|---|
    /// | PumpSwap | **before** | 55.7% of consecutive deltas match the *previous* swap's input, 0.4% match their own |
    /// | Pumpfun | **after** | 78.2% own / 3.1% previous |
    /// | Meteora DAMM v2 | **after** | 83.2% own / 0.3% previous |
    /// | Meteora DLMM | **both** | `start_bin_id` / `end_bin_id` are in the event |
    ///
    /// The unmatched remainder is fees leaving the vault, intervening
    /// liquidity events, and pool touches by instructions we do not decode.
    pub state_before: Option<CurveState>,
    /// Curve state immediately **after** this swap, when the protocol's event
    /// log reports it. See [`state_before`](Self::state_before) — the two are
    /// deliberately separate because protocols disagree about which one they
    /// publish, and a single field forces whichever extractor guessed wrong to
    /// lie.
    pub state_after: Option<CurveState>,
}

impl Swap {
    /// Is WSOL the [`token_in`](Self::token_in) side? `None` when the swap
    /// isn't SOL-paired at all.
    fn sol_is_in_side(&self) -> Option<bool> {
        if self.token_in == crate::tokens::WSOL {
            Some(true)
        } else if self.token_out == crate::tokens::WSOL {
            Some(false)
        } else {
            None
        }
    }

    /// **Spot price before** this swap, in lamports per raw token-side unit.
    pub fn sol_spot_price_before(&self) -> Option<f64> {
        self.state_before?.sol_price(self.sol_is_in_side()?)
    }

    /// **Spot price after** this swap, in lamports per raw token-side unit.
    pub fn sol_spot_price_after(&self) -> Option<f64> {
        self.state_after?.sol_price(self.sol_is_in_side()?)
    }

    /// The freshest spot price this swap can attest to — `after` when the
    /// protocol publishes it, otherwise `before`.
    ///
    /// This is the *curve's* price, **not** the effective price this trade
    /// paid (`amount_in / amount_out`). Use it for rolling-window analytics,
    /// mark-to-market, or feature extraction: the effective price is
    /// vulnerable to dust trades (1-lamport-for-100-raw-token swaps report
    /// wildly inflated effective prices) while the spot price is immune
    /// because dust barely moves the curve.
    ///
    /// Prefer [`sol_spot_price_before`] / [`sol_spot_price_after`] when the
    /// timing matters — on a protocol publishing only the pre-swap side, this
    /// value is one trade stale, which is precisely the error that made the
    /// old single `reserves_after` field wrong.
    ///
    /// `None` when the protocol publishes no curve state, the swap isn't
    /// SOL-paired, the token reserve is zero, or the state is a bin/tick
    /// index needing pool config to price.
    ///
    /// [`sol_spot_price_before`]: Self::sol_spot_price_before
    /// [`sol_spot_price_after`]: Self::sol_spot_price_after
    pub fn sol_spot_price_latest(&self) -> Option<f64> {
        self.sol_spot_price_after()
            .or_else(|| self.sol_spot_price_before())
    }

    /// Fractional price impact of this swap: `after / before - 1`.
    ///
    /// `None` unless the protocol publishes **both** sides — today only
    /// Meteora DLMM does, and its state is a bin index, so this is `None`
    /// there too until a `bin_step`-aware caller prices it. Filling the
    /// missing side for constant-product protocols is a tape-level pass
    /// (each swap's `state_after` is the next swap's `state_before` on the
    /// same pool), deliberately not done per-swap here.
    pub fn sol_price_impact(&self) -> Option<f64> {
        let before = self.sol_spot_price_before()?;
        let after = self.sol_spot_price_after()?;
        (before != 0.0).then(|| after / before - 1.0)
    }
}

/// The pricing curve's state at one point in a swap's lifetime.
///
/// Which variant a protocol uses is a property of its AMM *family*, not of the
/// swap: constant-product pools price off a reserve ratio, discrete-bin pools
/// off an integer bin index, concentrated-liquidity pools off a stored
/// square-root price. Only the first needs reserves at all — which is why bin
/// and tick protocols report price more directly, not less.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CurveState {
    /// Constant-product reserves, oriented to the parent swap's
    /// [`token_in`]/[`token_out`] so no external orientation is needed. Both
    /// values are in the respective mint's smallest units.
    ///
    /// * **Bonding curves (Pumpfun, Raydium Launchpad, Meteora DBC)** —
    ///   *virtual* reserves from the event log (what the pricing curve uses).
    ///   Real reserves evolve linearly off these.
    /// * **AMMs (PumpSwap, Meteora DAMM v2)** — real vault balances; the
    ///   pool's constant-product math uses them directly.
    ///
    /// [`token_in`]: Swap::token_in
    /// [`token_out`]: Swap::token_out
    Reserves {
        /// Pool reserve of [`Swap::token_in`].
        in_side: u64,
        /// Pool reserve of [`Swap::token_out`].
        out_side: u64,
    },
    /// Active bin index (Meteora DLMM).
    ///
    /// Not a price on its own: the conversion is
    /// `(1 + bin_step / 10_000) ^ bin_id`, and `bin_step` lives on the
    /// `LbPair` account. See `LbPair::price_at`.
    Bin(i32),
    /// √P in Q64.64 (concentrated liquidity). Price is `(x / 2^64)^2`.
    ///
    /// No extractor emits this yet — it exists so wiring a CLMM does not have
    /// to reshape the type.
    SqrtPriceX64(u128),
}

impl CurveState {
    /// SOL price per raw token unit, given which side of the parent swap is
    /// WSOL.
    ///
    /// `None` for variants that need pool configuration this type does not
    /// carry (a bin index needs its pair's `bin_step`).
    fn sol_price(self, sol_is_in_side: bool) -> Option<f64> {
        match self {
            Self::Reserves { in_side, out_side } => {
                let (sol, token) = if sol_is_in_side {
                    (in_side, out_side)
                } else {
                    (out_side, in_side)
                };
                (token != 0).then(|| sol as f64 / token as f64)
            }
            Self::Bin(_) | Self::SqrtPriceX64(_) => None,
        }
    }
}

/// A new token created on a bonding-curve protocol.
///
/// Today: emitted for Pumpfun `Create` (and Raydium Launchpad, when we
/// wire its extractor). Not emitted for plain AMM pool creations —
/// those are a `Migration` or simply out of scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCreation {
    pub protocol: Protocol,
    pub mint: Pubkey,
    /// Bonding curve or initial pool for the mint.
    pub pool: Pubkey,
    pub creator: Pubkey,
    pub name: String,
    pub symbol: String,
    /// Metadata URI (IPFS / HTTPS). Empty string when the protocol
    /// doesn't supply one.
    pub uri: String,
}

/// A token's liquidity graduating from one protocol to another.
///
/// Today: Pumpfun bonding curve → PumpSwap AMM pool. The bonding curve
/// account is closed / completed, and a fresh AMM pool is funded with
/// the remaining reserves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub mint: Pubkey,
    pub from_protocol: Protocol,
    pub from_pool: Pubkey,
    pub to_protocol: Protocol,
    pub to_pool: Pubkey,
    /// Lamports of SOL that moved from the old pool into the new one.
    pub migrated_sol: u64,
    /// Tokens that moved from the old pool into the new one.
    pub migrated_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    fn sample_swap() -> Swap {
        // Buy: trader paid WSOL (token_in), received mint token (token_out).
        Swap {
            track_volume: crate::protocols::OptionBool::None,
            instruction: crate::swap_instruction::SwapInstruction::Unknown([0; 8]),
            protocol: Protocol::Pumpfun,
            pool: pk(0x01),
            trader: pk(0x03),
            token_in: crate::tokens::WSOL,
            amount_in: 1_000_000_000,
            token_out: pk(0x02),
            amount_out: 33_333_333_333_333,
            fee_amount: 12_500_000,
            fee_mint: crate::tokens::WSOL,
            state_before: None,
            state_after: Some(CurveState::Reserves {
                in_side: 31_000_000_000,
                out_side: 966_666_666_666_667,
            }),
        }
    }

    fn sample_tx(events: Vec<ChainEvent>, outcome: TxOutcome) -> ParsedTransaction {
        ParsedTransaction {
            signature: Signature::default(),
            slot: 42,
            index: 0,
            block_time: None,
            fee_paid_lamports: 5_000,
            compute_used: Some(200_000),
            outcome,
            events,
            instructions: Vec::new(),
            token_balances: Vec::new(),
        }
    }

    fn entry(idx: u32, owner: u8, mint: u8, raw: u64) -> TokenBalanceEntry {
        TokenBalanceEntry {
            account_index: idx,
            program: pk(0xEE),
            owner: pk(owner),
            mint: pk(mint),
            raw,
            decimals: 6,
        }
    }

    #[test]
    fn pair_matches_pre_and_post_by_index() {
        let out = TokenBalanceChange::pair(
            vec![entry(3, 0xA, 0xB, 100), entry(5, 0xC, 0xB, 7)],
            vec![entry(3, 0xA, 0xB, 250), entry(5, 0xC, 0xB, 0)],
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].pre_raw, Some(100));
        assert_eq!(out[0].post_raw, Some(250));
        assert_eq!(out[0].delta_raw(), 150);
        // A reported zero stays Some(0), never None.
        assert_eq!(out[1].post_raw, Some(0));
        assert_eq!(out[1].delta_raw(), -7);
    }

    #[test]
    fn pair_post_only_is_created_account() {
        let out = TokenBalanceChange::pair(vec![], vec![entry(2, 0xA, 0xB, 42)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pre_raw, None);
        assert_eq!(out[0].post_raw, Some(42));
        assert_eq!(out[0].delta_raw(), 42);
    }

    #[test]
    fn pair_pre_only_is_closed_account() {
        let out = TokenBalanceChange::pair(vec![entry(2, 0xA, 0xB, 42)], vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pre_raw, Some(42));
        assert_eq!(out[0].post_raw, None);
    }

    #[test]
    fn pair_same_index_different_identity_yields_two_unpaired() {
        // Account at index 4 closed and re-created as a different ATA
        // within one tx: a paired delta would lie about both.
        let out =
            TokenBalanceChange::pair(vec![entry(4, 0xA, 0xB, 10)], vec![entry(4, 0xD, 0xE, 20)]);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].pre_raw, out[0].post_raw), (Some(10), None));
        assert_eq!(out[0].owner, pk(0xA));
        assert_eq!((out[1].pre_raw, out[1].post_raw), (None, Some(20)));
        assert_eq!(out[1].owner, pk(0xD));
    }

    #[test]
    fn swaps_iterator_filters_to_swap_variant_only() {
        let tx = sample_tx(
            vec![
                ChainEvent::Swap(sample_swap()),
                ChainEvent::TokenCreation(TokenCreation {
                    protocol: Protocol::Pumpfun,
                    mint: pk(0x02),
                    pool: pk(0x01),
                    creator: pk(0x03),
                    name: "Test".into(),
                    symbol: "TST".into(),
                    uri: String::new(),
                }),
                ChainEvent::Swap(sample_swap()),
            ],
            TxOutcome::Success,
        );

        assert_eq!(tx.swaps().count(), 2);
        assert_eq!(tx.token_creations().count(), 1);
        assert_eq!(tx.migrations().count(), 0);
    }

    #[test]
    fn succeeded_reflects_outcome_variant() {
        let ok = sample_tx(vec![], TxOutcome::Success);
        assert!(ok.succeeded());

        let failed = sample_tx(vec![], TxOutcome::Failed(TxError::Slippage));
        assert!(!failed.succeeded());
    }

    #[test]
    fn chain_event_protocol_routes_through_variant() {
        let swap = ChainEvent::Swap(sample_swap());
        assert_eq!(swap.protocol(), Protocol::Pumpfun);

        let migration = ChainEvent::Migration(Migration {
            mint: pk(0x02),
            from_protocol: Protocol::Pumpfun,
            from_pool: pk(0x01),
            to_protocol: Protocol::PumpSwap,
            to_pool: pk(0x04),
            migrated_sol: 85_000_000_000,
            migrated_tokens: 206_900_000_000_000,
        });
        assert_eq!(migration.protocol(), Protocol::Pumpfun);
    }

    #[test]
    fn parsed_transaction_roundtrips_through_bincode() {
        // Smoke test for serde derives — the types should be bincode-friendly
        // since they flow through cache persistence / broadcast channels.
        let tx = sample_tx(vec![ChainEvent::Swap(sample_swap())], TxOutcome::Success);
        let bytes = bincode::serialize(&tx).expect("serialize");
        let decoded: ParsedTransaction = bincode::deserialize(&bytes).expect("deserialize");

        assert_eq!(decoded.slot, tx.slot);
        assert_eq!(decoded.events.len(), 1);
        assert!(decoded.succeeded());
    }

    #[test]
    fn tx_error_slippage_is_distinguishable() {
        // Reactive-trader branches on this; make sure pattern matching works
        // without the consumer needing to stringify.
        let err = TxError::Slippage;
        let is_slippage = matches!(err, TxError::Slippage);
        assert!(is_slippage);

        let other = TxError::Rejected {
            program_id: pk(0xAA),
            custom_code: 6001,
        };
        let is_slippage = matches!(other, TxError::Slippage);
        assert!(!is_slippage);
    }
}
