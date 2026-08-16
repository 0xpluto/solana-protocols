//! Pumpfun extractor — transforms parsed instructions into
//! semantic [`ChainEvent`]s.
//!
//! Pumpfun is the reference "bonding-curve with Anchor `emit_cpi!`"
//! protocol. The `TradeEvent` log carries every field a [`Swap`]
//! needs — executed amounts, `is_buy`, user, mint, post-trade
//! reserves — so no CPI-transfer reconciliation is required and
//! [`ExtractContext`] goes unused.
//!
//! Later bonding-curve protocols (Raydium Launchpad, Meteora DBC)
//! share this shape; the log-event helper pattern here will
//! generalise.

use solana_program::pubkey::Pubkey;
use tracing::warn;

use super::events::{
    CollectCreatorFeeEvent, DistributeCreatorFeesEvent, TradeEvent, TRADE_EVENT_DISCRIMINATOR,
};
use super::{
    BuyAccounts, CreateAccounts, CreateParams, CreateV2Accounts, CreateV2Params,
    PumpfunInstruction, SellAccounts, PROGRAM_ID as PUMPFUN_PROGRAM,
};
use crate::chain::{
    ChainEvent, CreatorFee, CreatorPayout, CurveState, ExtractContext, ProtocolExtractor, Swap,
    TokenCreation,
};
use crate::parsing::anchor::{event_body, ANCHOR_EVENT_TAG};
use crate::parsing::event::find_child_event;
use crate::parsing::{FromAccountKeys, ParsedInstruction};
use crate::protocols::Protocol;

/// Zero-sized adapter. Register via
/// [`ExtractorRegistry::register`](crate::chain::ExtractorRegistry::register).
pub struct PumpfunExtractor;

impl ProtocolExtractor for PumpfunExtractor {
    fn program_id() -> Pubkey {
        PUMPFUN_PROGRAM
    }

    fn extract(
        ix: &ParsedInstruction,
        all_instructions: &[ParsedInstruction],
        _ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        // Pumpfun emits `TradeEvent` via Anchor `emit_cpi!`, so the
        // event payload lives on an *inner* self-CPI ix (the
        // event-authority CPI), not on the outer Buy/Sell ix. Two
        // kinds of pumpfun ixs reach this extractor:
        //
        // * Outer Buy/Sell/Create — `ix.data` starts with the
        //   instruction discriminator. We pull mint / pool / user
        //   from `ix.accounts` and walk children to find the
        //   matching `TradeEvent` log.
        // * Inner event self-CPIs — `ix.data` starts with the
        //   8-byte Anchor event tag. We skip these here; the
        //   parent Buy/Sell ix is what produces the chain event.
        //
        // The skip keys on the *tag*, not on TradeEvent's own
        // discriminator: every Anchor event self-CPI must be skipped
        // here, not just trades.
        if ix.data.len() >= 8 && ix.data[..8] == ANCHOR_EVENT_TAG {
            return None;
        }

        let pumpfun_ix = match PumpfunInstruction::try_from_slice(&ix.data) {
            Ok(v) => v,
            Err(e) => {
                // Not "skipping" — this is an instruction on a program we CLAIM
                // to decode, so failing to parse it is a gap in us, not a
                // non-event. Retained with its bytes so the parser can be
                // fixed later; this branch used to `trace!` and vanish.
                crate::undecoded::report(&ix.program_id, &ix.data, &ix.accounts, &format!("{e:?}"));
                return None;
            }
        };

        match pumpfun_ix {
            // All six swap forms route here; `extract_swap` decides which can
            // be decoded from their account layout and skips the rest loudly.
            PumpfunInstruction::Buy(_)
            | PumpfunInstruction::BuyV2(_)
            | PumpfunInstruction::BuyExactSolIn(_)
            | PumpfunInstruction::BuyExactQuoteInV2(_)
            | PumpfunInstruction::Sell(_)
            | PumpfunInstruction::SellV2(_) => extract_swap(ix, &pumpfun_ix, all_instructions),
            PumpfunInstruction::Create(params) => extract_create(ix, &params),
            PumpfunInstruction::CreateV2(params) => extract_create_v2(ix, &params),
            PumpfunInstruction::CollectCreatorFee(_)
            | PumpfunInstruction::CollectCreatorFeeV2(_) => {
                extract_collect_creator_fee(ix, all_instructions)
            }
            PumpfunInstruction::DistributeCreatorFees(_)
            | PumpfunInstruction::DistributeCreatorFeesV2(_) => {
                extract_distribute_creator_fees(ix, all_instructions)
            }
        }
    }
}

fn extract_swap(
    ix: &ParsedInstruction,
    pumpfun_ix: &PumpfunInstruction,
    all_instructions: &[ParsedInstruction],
) -> Option<ChainEvent> {
    // Instruction accounts give us the pool identity (mint /
    // bonding_curve / user). Executed amounts come from the TradeEvent
    // log — declared params are slippage bounds we discard.
    let (mint, pool, trader) = match pumpfun_ix {
        // `buy_exact_sol_in` carries `buy`'s account layout exactly (it differs
        // only in discriminator and which side the params pin), so it decodes
        // through the same struct. Which instruction ran is preserved on the
        // row, so the two are never graded as one.
        PumpfunInstruction::Buy(_) | PumpfunInstruction::BuyExactSolIn(_) => {
            let accounts = match BuyAccounts::from_account_keys(&ix.accounts) {
                Ok(a) => a,
                Err(e) => {
                    warn!(
                        ?e,
                        ix_index = ix.instruction_index,
                        "pumpfun buy: account parse failed"
                    );
                    return None;
                }
            };
            (accounts.mint, accounts.bonding_curve, accounts.user)
        }
        // The v2 instructions carry a different, and *variable*, account
        // layout — observed at 26/27/28/29 slots on mainnet 2026-08-12, so no
        // fixed index is safe and decoding them through the v1 structs would
        // silently yield the wrong mint, pool and trader.
        //
        // They do not need one. The `TradeEvent` already names the mint and
        // the trader, and the bonding curve is a PDA *of* the mint — so
        // identity is recovered without reference to any slot. That is
        // strictly more robust than the v1 path: it cannot be broken by the
        // program reordering, adding, or removing accounts, and the derived
        // PDA is self-verifying because it must appear in the instruction's
        // own account list.
        PumpfunInstruction::BuyV2(_)
        | PumpfunInstruction::BuyExactQuoteInV2(_)
        | PumpfunInstruction::SellV2(_) => {
            let ev = find_trade_event(ix, all_instructions)?;
            let pool = super::accounts::derive_bonding_curve_pda(&ev.mint);
            if !ix.accounts.contains(&pool) {
                warn!(
                    mint = %ev.mint,
                    derived_pool = %pool,
                    ix_index = ix.instruction_index,
                    "pumpfun v2: bonding curve derived from the event mint is \
                     absent from the instruction's accounts — refusing to \
                     record an identity we cannot corroborate"
                );
                return None;
            }
            (ev.mint, pool, ev.user)
        }
        PumpfunInstruction::Sell(_) => {
            let accounts = match SellAccounts::from_account_keys(&ix.accounts) {
                Ok(a) => a,
                Err(e) => {
                    warn!(
                        ?e,
                        ix_index = ix.instruction_index,
                        "pumpfun sell: account parse failed"
                    );
                    return None;
                }
            };
            (accounts.mint, accounts.bonding_curve, accounts.user)
        }
        PumpfunInstruction::Create(_)
        | PumpfunInstruction::CreateV2(_)
        | PumpfunInstruction::CollectCreatorFee(_)
        | PumpfunInstruction::CollectCreatorFeeV2(_)
        | PumpfunInstruction::DistributeCreatorFees(_)
        | PumpfunInstruction::DistributeCreatorFeesV2(_) => return None,
    };

    let trade_event = find_trade_event(ix, all_instructions)?;

    // Sanity check: the mint in the log must match the mint the
    // instruction targets. Mismatch = tampered log or parser bug.
    if trade_event.mint != mint {
        warn!(
            log_mint = %trade_event.mint,
            account_mint = %mint,
            "pumpfun TradeEvent mint mismatches instruction accounts"
        );
    }

    // Pumpfun bonding curves are always SOL-denominated on one side.
    // Map the executed amounts + virtual reserves into the
    // token-agnostic Swap shape relative to trade direction.
    let (token_in, amount_in, token_out, amount_out, reserve_in, reserve_out) =
        if trade_event.is_buy {
            // Trader paid SOL, received the bonding-curve token.
            (
                crate::tokens::WSOL,
                trade_event.sol_amount,
                mint,
                trade_event.token_amount,
                trade_event.virtual_sol_reserves,
                trade_event.virtual_token_reserves,
            )
        } else {
            // Trader paid the token, received SOL.
            (
                mint,
                trade_event.token_amount,
                crate::tokens::WSOL,
                trade_event.sol_amount,
                trade_event.virtual_token_reserves,
                trade_event.virtual_sol_reserves,
            )
        };

    Some(ChainEvent::Swap(Swap {
        // Read off the instruction rather than defaulted: on
        // `buy_exact_quote_in_v2` this is an argument the IDL does not declare,
        // and it is the only record that the trade opted in.
        track_volume: match pumpfun_ix {
            PumpfunInstruction::Buy(p) | PumpfunInstruction::BuyV2(p) => p.track_volume,
            PumpfunInstruction::BuyExactSolIn(p) | PumpfunInstruction::BuyExactQuoteInV2(p) => {
                p.track_volume
            }
            PumpfunInstruction::Sell(_)
            | PumpfunInstruction::SellV2(_)
            | PumpfunInstruction::Create(_)
            | PumpfunInstruction::CreateV2(_)
            | PumpfunInstruction::CollectCreatorFee(_)
            | PumpfunInstruction::CollectCreatorFeeV2(_)
            | PumpfunInstruction::DistributeCreatorFees(_)
            | PumpfunInstruction::DistributeCreatorFeesV2(_) => crate::protocols::OptionBool::None,
        },
        instruction: crate::swap_instruction::resolve(&ix.program_id, &ix.data),
        protocol: Protocol::Pumpfun,
        pool,
        trader,
        token_in,
        amount_in,
        token_out,
        amount_out,
        // Read off the event, not recomputed: the chain publishes the exact
        // protocol + creator lamports it charged. An `Absent` fee block means
        // the event predates those fields, which is the only case that still
        // records zero. Pumpfun charges in SOL regardless of direction.
        fee_amount: trade_event.fee.saturating_add(trade_event.creator_fee),
        fee_mint: crate::tokens::WSOL,
        state_before: None,
        state_after: Some(CurveState::Reserves {
            in_side: reserve_in,
            out_side: reserve_out,
        }),
    }))
}

/// Legacy v1 `create` extraction. 14-slot account layout; user
/// (creator) at slot 7.
fn extract_create(ix: &ParsedInstruction, params: &CreateParams) -> Option<ChainEvent> {
    let accounts = match CreateAccounts::from_account_keys(&ix.accounts) {
        Ok(a) => a,
        Err(e) => {
            warn!(?e, "pumpfun create v1: account parse failed");
            return None;
        }
    };

    Some(ChainEvent::TokenCreation(TokenCreation {
        protocol: Protocol::Pumpfun,
        mint: accounts.mint,
        pool: accounts.bonding_curve,
        creator: accounts.user,
        name: params.name.clone(),
        symbol: params.symbol.clone(),
        uri: params.uri.clone(),
    }))
}

/// Modern v2 `create_v2` extraction. 16-slot layout; user
/// (signer/payer) at slot 5. The instruction's args also carry an
/// explicit `creator: Pubkey` field — used as the canonical creator
/// since pumpfun records that on-chain. Falls back to
/// `accounts.user` when the args creator is the default pubkey
/// (rare; would indicate a malformed instruction).
fn extract_create_v2(ix: &ParsedInstruction, params: &CreateV2Params) -> Option<ChainEvent> {
    let accounts = match CreateV2Accounts::from_account_keys(&ix.accounts) {
        Ok(a) => a,
        Err(e) => {
            warn!(?e, "pumpfun create_v2: account parse failed");
            return None;
        }
    };

    // Prefer the explicit `creator` arg over the signer when it's
    // populated — it's what pumpfun stores as canonical. They almost
    // always match in practice; the rare divergence comes from
    // launch-service proxies signing on behalf of an end user.
    let creator = if params.creator == solana_program::pubkey::Pubkey::default() {
        accounts.user
    } else {
        params.creator
    };

    Some(ChainEvent::TokenCreation(TokenCreation {
        protocol: Protocol::Pumpfun,
        mint: accounts.mint,
        pool: accounts.bonding_curve,
        creator,
        name: params.name.clone(),
        symbol: params.symbol.clone(),
        uri: params.uri.clone(),
    }))
}

/// Find the `TradeEvent` self-CPI for the given Buy/Sell ix.
///
/// Modern pumpfun emits `TradeEvent` via Anchor `emit_cpi!`. The
/// payload arrives as an **inner self-CPI** ix whose data is:
///
/// ```text
/// [ 0..8 ] ANCHOR_EVENT_TAG              — every Anchor event, any program
/// [ 8..16] TRADE_EVENT_DISCRIMINATOR     — which event
/// [16.. ] borsh-serialised TradeEvent body
/// ```
///
/// Legacy pumpfun used `emit!`, where the payload sits on the outer
/// ix's `Program data:` log as `[TRADE_EVENT_DISCRIMINATOR || body]`
/// — the event's own discriminator, no tag. We support both.
///
/// Both layers are checked on the modern path. Matching the tag alone
/// says only "some Anchor event"; a 2026-08-11 sample of 25 mainnet
/// transactions found 9 event self-CPIs (all TradeEvent) and a second,
/// distinct discriminator in the same `Program data:` channel — so the
/// `[8..16]` check is what keeps a sibling event out of the swap tape.
/// A creator withdrawing their accrued fees.
///
/// Everything recorded comes from the event, not the instruction: the
/// instruction takes no arguments, and mainnet sends it with more accounts than
/// the IDL declares, so slot-reading it would be guessing. No event means no
/// row — a withdrawal whose amount we cannot read is not worth a fabricated
/// zero.
fn extract_collect_creator_fee(
    ix: &ParsedInstruction,
    all_instructions: &[ParsedInstruction],
) -> Option<ChainEvent> {
    let ev: CollectCreatorFeeEvent = find_child_event(ix, all_instructions, &PUMPFUN_PROGRAM)
        .or_else(|| {
            warn!(
                ix_index = ix.instruction_index,
                "pumpfun collect_creator_fee: no CollectCreatorFeeEvent on any child"
            );
            None
        })?;

    Some(ChainEvent::CreatorFee(CreatorFee {
        protocol: Protocol::Pumpfun,
        payout: CreatorPayout::Direct {
            creator: ev.creator,
        },
        amount: ev.creator_fee,
        quote_mint: ev.quote_mint,
        // The vault accrues across every token this creator launched, so the
        // chain genuinely does not attribute this to one mint.
        mint: None,
        timestamp: ev.timestamp,
    }))
}

/// Accrued fees split across a sharing config.
///
/// Unlike a collect this one *is* attributable — the event names the mint and
/// the bonding curve whose trading earned the fees.
fn extract_distribute_creator_fees(
    ix: &ParsedInstruction,
    all_instructions: &[ParsedInstruction],
) -> Option<ChainEvent> {
    let ev: DistributeCreatorFeesEvent = find_child_event(ix, all_instructions, &PUMPFUN_PROGRAM)
        .or_else(|| {
        warn!(
            ix_index = ix.instruction_index,
            "pumpfun distribute_creator_fees: no DistributeCreatorFeesEvent on any child"
        );
        None
    })?;

    Some(ChainEvent::CreatorFee(CreatorFee {
        protocol: Protocol::Pumpfun,
        payout: CreatorPayout::Shared {
            bonding_curve: ev.bonding_curve,
            sharing_config: ev.sharing_config,
            admin: ev.admin,
            shareholders: ev.shareholders,
        },
        amount: ev.distributed,
        quote_mint: ev.quote_mint,
        mint: Some(ev.mint),
        timestamp: ev.timestamp,
    }))
}

fn find_trade_event(
    ix: &ParsedInstruction,
    all_instructions: &[ParsedInstruction],
) -> Option<TradeEvent> {
    let outer_idx = ix.instruction_index;

    // Modern: walk children for a pumpfun self-CPI whose data starts
    // with the Anchor event envelope.
    for child in all_instructions.iter() {
        if child.parent_index != Some(outer_idx) {
            continue;
        }
        if child.program_id != PUMPFUN_PROGRAM {
            continue;
        }
        let Some(body) = event_body(&child.data, &TRADE_EVENT_DISCRIMINATOR) else {
            continue;
        };
        if let Some(ev) = parse_trade_event_body(body) {
            return Some(ev);
        }
    }

    // Legacy `emit!` fallback — payload on the outer ix's log stream,
    // where Anchor writes `[event disc || body]` with no tag.
    let payload = ix.find_data_log_with_discriminator(&TRADE_EVENT_DISCRIMINATOR)?;
    parse_trade_event_body(payload)
}

/// Decode a `TradeEvent` body.
///
/// Borsh, against the field list the program's own IDL declares — 32 fields,
/// where the hand-written predecessor read the first 121 bytes and stopped.
/// That is why `ix_name`, the executed `track_volume`, the cashback and
/// buyback splits and the shareholder table were all invisible: they sit past
/// the offset it gave up at.
fn parse_trade_event_body(body: &[u8]) -> Option<TradeEvent> {
    use crate::parsing::event::ProtocolEvent;
    TradeEvent::from_event_body(body)
        .inspect_err(|e| warn!(len = body.len(), %e, "pumpfun TradeEvent body decode failed"))
        .ok()
}

// =============================================================================
// Tests — Pumpfun-specific only. Cross-protocol orchestration tests live
// in `chain/extract/mod.rs`.
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::NoContext;
    use crate::parsing::{LogEntry, ParsedInstructionBuilder};
    use crate::protocols::pumpfun::{BUY_DISCRIMINATOR, CREATE_DISCRIMINATOR};

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    /// The event body, serialised through borsh.
    ///
    /// Was a hand-packed 121-byte layout. Now that the struct IS the layout,
    /// round-tripping through borsh means a test body cannot disagree with the
    /// decoder — the failure mode where a test enshrines a stale layout and
    /// stays green against real data that no longer matches.
    fn trade_event_body(event: &TradeEvent) -> Vec<u8> {
        borsh::to_vec(event).expect("TradeEvent serialises")
    }

    /// Modern `emit_cpi!` framing: `[tag || event disc || body]`.
    fn encode_trade_event(event: &TradeEvent) -> Vec<u8> {
        emit_cpi_framed(&TRADE_EVENT_DISCRIMINATOR, &trade_event_body(event))
    }

    /// Modern framing under an arbitrary event discriminator, so a test can
    /// pose as a *different* Anchor event from the same program.
    fn emit_cpi_framed(disc: &[u8; 8], body: &[u8]) -> Vec<u8> {
        let mut data = Vec::with_capacity(16 + body.len());
        data.extend_from_slice(&ANCHOR_EVENT_TAG);
        data.extend_from_slice(disc);
        data.extend_from_slice(body);
        data
    }

    /// Legacy `emit!` framing: `[event disc || body]` on a `Program data:` log.
    fn encode_trade_event_log(event: &TradeEvent) -> Vec<u8> {
        trade_event_body(event)
    }

    fn sample_trade_event(is_buy: bool) -> TradeEvent {
        TradeEvent {
            mint: pk(0xAA),
            sol_amount: 1_000_000_000,
            token_amount: 33_000_000_000_000,
            is_buy,
            user: pk(0xBB),
            timestamp: 1_700_000_000,
            virtual_sol_reserves: 31_000_000_000,
            virtual_token_reserves: 967_000_000_000_000,
            real_sol_reserves: 1_000_000_000,
            real_token_reserves: 767_000_000_000_000,
            ..Default::default()
        }
    }

    /// Full 16-slot BuyAccounts layout. Only mint/bonding_curve/user
    /// are inspected; other slots just need to be present so
    /// `from_account_keys` accepts the slice.
    fn buy_accounts_with(mint: Pubkey, bonding_curve: Pubkey, user: Pubkey) -> Vec<Pubkey> {
        vec![
            pk(0x01),
            pk(0x02),
            mint,
            bonding_curve,
            pk(0x05),
            pk(0x06),
            user,
            pk(0x08),
            pk(0x09),
            pk(0x0A),
            pk(0x0B),
            pk(0x0C),
            pk(0x0D),
            pk(0x0E),
            pk(0x0F),
            pk(0x10),
        ]
    }

    fn buy_instruction(event: &TradeEvent, params_data: Vec<u8>) -> ParsedInstruction {
        let pool = pk(0xCC);

        let mut ix_data = Vec::new();
        ix_data.extend_from_slice(&BUY_DISCRIMINATOR);
        ix_data.extend_from_slice(&params_data);

        let accounts = buy_accounts_with(event.mint, pool, event.user);
        let trade_log_payload = encode_trade_event_log(event);

        ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(accounts)
            .data(ix_data)
            .log(LogEntry::Data {
                discriminator: TRADE_EVENT_DISCRIMINATOR,
                payload: trade_log_payload,
            })
            .stack_height(1)
            .instruction_index(0)
            .build()
    }

    #[test]
    fn buy_produces_swap_event_with_executed_amounts_from_log() {
        let event = sample_trade_event(true);
        let params_data = {
            let mut v = Vec::with_capacity(16);
            // Declared tokens_out + max_sol_cost — slippage bounds we
            // deliberately discard in favour of log amounts.
            v.extend_from_slice(&999_999u64.to_le_bytes());
            v.extend_from_slice(&2_000_000_000u64.to_le_bytes());
            v
        };
        let ix = buy_instruction(&event, params_data);

        let result = PumpfunExtractor::extract(&ix, std::slice::from_ref(&ix), &NoContext)
            .expect("expected Swap event");
        let swap = match result {
            ChainEvent::Swap(s) => s,
            other => panic!("expected Swap, got {other:?}"),
        };
        assert_eq!(swap.protocol, Protocol::Pumpfun);
        // Buy = SOL in, token out.
        assert_eq!(swap.token_in, crate::tokens::WSOL);
        assert_eq!(swap.amount_in, event.sol_amount);
        assert_eq!(swap.token_out, event.mint);
        assert_eq!(swap.amount_out, event.token_amount);
        assert_eq!(swap.trader, event.user);
        assert_eq!(swap.fee_mint, crate::tokens::WSOL);
        // Pumpfun publishes the POST-swap side (measured: 78.2% of consecutive
        // deltas match the swap's own amounts vs 3.1% the previous swap's).
        assert_eq!(swap.state_before, None);
        assert_eq!(
            swap.state_after,
            // in_side = SOL side reserve (for a Buy), out_side = token side.
            Some(CurveState::Reserves {
                in_side: event.virtual_sol_reserves,
                out_side: event.virtual_token_reserves,
            })
        );
    }

    #[test]
    fn swap_without_trade_log_is_skipped() {
        let event = sample_trade_event(true);
        let params_data = vec![0u8; 16];
        let mut ix = buy_instruction(&event, params_data);
        ix.logs.clear();

        assert!(PumpfunExtractor::extract(&ix, std::slice::from_ref(&ix), &NoContext).is_none());
    }

    /// Modern pumpfun emits TradeEvent via Anchor `emit_cpi!` — the
    /// payload arrives as an inner self-CPI ix whose data is
    /// `ANCHOR_EVENT_DISC || borsh body`. Verify the extractor walks
    /// children and finds it.
    #[test]
    fn buy_with_emit_cpi_event_finds_trade_via_inner_ix() {
        let event = sample_trade_event(true);
        let params_data = {
            let mut v = Vec::with_capacity(16);
            v.extend_from_slice(&0u64.to_le_bytes());
            v.extend_from_slice(&0u64.to_le_bytes());
            v
        };
        let mut outer = buy_instruction(&event, params_data);
        // Drop the legacy log — force the extractor through the
        // self-CPI path.
        outer.logs.clear();

        // Inner self-CPI: same program_id, parent points at outer,
        // data is the full encoded TradeEvent (disc + payload).
        let mut inner = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(vec![pk(0x0B), pk(0x0C)]) // event_authority, program — unused
            .data(encode_trade_event(&event))
            .stack_height(2)
            .instruction_index(1)
            .build();
        inner.parent_index = Some(0);

        let all = vec![outer.clone(), inner];
        let result = PumpfunExtractor::extract(&all[0], &all, &NoContext).expect("expected Swap");
        match result {
            ChainEvent::Swap(s) => {
                assert_eq!(s.amount_in, event.sol_amount);
                assert_eq!(s.amount_out, event.token_amount);
                assert_eq!(s.token_out, event.mint);
            }
            other => panic!("expected Swap, got {other:?}"),
        }
    }

    /// A sibling Anchor event from pumpfun must **not** be read as a trade.
    ///
    /// Both events carry the same `ANCHOR_EVENT_TAG`; only bytes `[8..16]`
    /// tell them apart. This test replaced one named
    /// `..._skips_event_name_disc`, which asserted the extractor ignored that
    /// range — the defect written down as the specification. Reverting
    /// `find_trade_event` to match on the tag alone fails this test.
    #[test]
    fn a_different_anchor_event_is_not_decoded_as_a_trade() {
        let event = sample_trade_event(true);
        let foreign_disc = [0xab; 8];
        assert_ne!(foreign_disc, TRADE_EVENT_DISCRIMINATOR);

        // Same program, same tag, same body length — everything a
        // tag-only match would accept.
        let inner_data = emit_cpi_framed(&foreign_disc, &trade_event_body(&event));
        // Length is whatever borsh produces for the full IDL layout; the
        // point is only that it is a plausible body, not a magic number.
        assert!(inner_data.len() > 16);

        let mut outer = buy_instruction(&event, vec![0u8; 16]);
        outer.logs.clear(); // no legacy log — the self-CPI path is the only one
        let mut inner = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(vec![pk(0x0B), pk(0x0C)])
            .data(inner_data)
            .stack_height(2)
            .instruction_index(1)
            .build();
        inner.parent_index = Some(0);

        let all = vec![outer, inner];
        assert!(
            PumpfunExtractor::extract(&all[0], &all, &NoContext).is_none(),
            "a foreign Anchor event was decoded as a TradeEvent"
        );
    }

    /// The modern framing decodes, and the amounts come from the body — the
    /// positive control for the test above, so a blanket "refuse everything"
    /// regression cannot pass both.
    #[test]
    fn the_modern_emit_cpi_framing_decodes_with_the_right_event_disc() {
        let event = sample_trade_event(true);
        let inner_data = emit_cpi_framed(&TRADE_EVENT_DISCRIMINATOR, &trade_event_body(&event));

        let mut outer = buy_instruction(&event, vec![0u8; 16]);
        outer.logs.clear();
        let mut inner = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(vec![pk(0x0B), pk(0x0C)])
            .data(inner_data)
            .stack_height(2)
            .instruction_index(1)
            .build();
        inner.parent_index = Some(0);

        let all = vec![outer, inner];
        match PumpfunExtractor::extract(&all[0], &all, &NoContext).expect("expected Swap") {
            ChainEvent::Swap(s) => {
                assert_eq!(s.amount_in, event.sol_amount);
                assert_eq!(s.amount_out, event.token_amount);
                assert_eq!(s.token_out, event.mint);
                assert_eq!(s.trader, event.user);
            }
            other => panic!("expected Swap, got {other:?}"),
        }
    }

    #[test]
    fn inner_event_cpi_does_not_emit_a_swap() {
        let event = sample_trade_event(true);
        let inner = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(vec![pk(0x0B), pk(0x0C)])
            .data(encode_trade_event(&event))
            .stack_height(2)
            .instruction_index(0)
            .build();
        assert!(
            PumpfunExtractor::extract(&inner, std::slice::from_ref(&inner), &NoContext).is_none()
        );
    }

    #[test]
    fn create_produces_token_creation_event() {
        let create = CreateParams::new(
            "Test Token".into(),
            "TST".into(),
            "https://example.com/meta.json".into(),
        );
        let mint = pk(0xAA);
        let bonding_curve = pk(0xCC);
        let creator = pk(0xBB);

        // 14-slot CreateAccounts: mint(0), bonding_curve(2), user(7)
        // are the fields we extract.
        let accounts = vec![
            mint,
            pk(0x01),
            bonding_curve,
            pk(0x03),
            pk(0x04),
            pk(0x05),
            pk(0x06),
            creator,
            pk(0x07),
            pk(0x08),
            pk(0x09),
            pk(0x0A),
            pk(0x0B),
            pk(0x0C),
        ];

        let ix = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(accounts)
            .data(create.to_data())
            .stack_height(1)
            .instruction_index(0)
            .build();

        let result = PumpfunExtractor::extract(&ix, std::slice::from_ref(&ix), &NoContext)
            .expect("expected TokenCreation");
        let creation = match result {
            ChainEvent::TokenCreation(c) => c,
            other => panic!("expected TokenCreation, got {other:?}"),
        };
        assert_eq!(creation.protocol, Protocol::Pumpfun);
        assert_eq!(creation.mint, mint);
        assert_eq!(creation.pool, bonding_curve);
        assert_eq!(creation.creator, creator);
        assert_eq!(creation.name, "Test Token");
        assert_eq!(creation.symbol, "TST");
        assert_eq!(creation.uri, "https://example.com/meta.json");

        // Catches silent refactors that break discriminator wiring.
        assert_eq!(&ix.data[..8], &CREATE_DISCRIMINATOR);
    }

    /// Modern pumpfun emits `create_v2` with a 16-slot account layout
    /// (user/creator at slot 5, not 7) and richer params (explicit
    /// `creator: Pubkey` arg + mayhem/cashback flags). Verify the
    /// extractor reads the right slot AND prefers the args.creator.
    #[test]
    fn create_v2_extracts_with_correct_layout_and_creator() {
        use crate::protocols::pumpfun::CREATE_V2_DISCRIMINATOR;

        let mint = pk(0xAA);
        let bonding_curve = pk(0xCC);
        let signer = pk(0xBB); // accounts.user (slot 5)
        let canonical_creator = pk(0xDD); // params.creator — preferred

        // Build the v2 ix data: discriminator + (name, symbol, uri,
        // creator, is_mayhem_mode, OptionBool).
        let mut ix_data = Vec::new();
        ix_data.extend_from_slice(&CREATE_V2_DISCRIMINATOR);
        for s in ["V2 Token", "V2", "https://v2.example"] {
            ix_data.extend_from_slice(&(s.len() as u32).to_le_bytes());
            ix_data.extend_from_slice(s.as_bytes());
        }
        ix_data.extend_from_slice(canonical_creator.as_ref());
        ix_data.push(0); // is_mayhem_mode = false
        ix_data.push(0); // OptionBool = None (no trailing byte)

        // 16-slot v2 layout: mint=0, bonding_curve=2, user=5,
        // token_program=7 (the program ID we used to mistake for
        // creator), event_authority=14, program=15.
        let accounts = vec![
            mint,            // 0  mint
            pk(0x01),        // 1  mint_authority
            bonding_curve,   // 2  bonding_curve
            pk(0x03),        // 3  associated_bonding_curve
            pk(0x04),        // 4  global
            signer,          // 5  user (signer)
            pk(0x06),        // 6  system_program
            pk(0x07),        // 7  token_program  ← v1 had user here
            pk(0x08),        // 8  associated_token_program
            pk(0x09),        // 9  mayhem_program_id
            pk(0x0A),        // 10 global_params
            pk(0x0B),        // 11 sol_vault
            pk(0x0C),        // 12 mayhem_state
            pk(0x0D),        // 13 mayhem_token_vault
            pk(0x0E),        // 14 event_authority
            PUMPFUN_PROGRAM, // 15 program
        ];

        let ix = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(accounts)
            .data(ix_data)
            .stack_height(1)
            .instruction_index(0)
            .build();

        let result = PumpfunExtractor::extract(&ix, std::slice::from_ref(&ix), &NoContext)
            .expect("expected TokenCreation from create_v2");
        let creation = match result {
            ChainEvent::TokenCreation(c) => c,
            other => panic!("expected TokenCreation, got {other:?}"),
        };
        assert_eq!(creation.mint, mint);
        assert_eq!(creation.pool, bonding_curve);
        // Canonical creator from the args, not the slot-7 token program.
        assert_eq!(creation.creator, canonical_creator);
        assert_eq!(creation.name, "V2 Token");
        assert_eq!(creation.symbol, "V2");
    }
}

#[cfg(test)]
mod trade_event_fixture {
    use super::*;

    /// A real mainnet `TradeEvent` decodes through borsh against the field
    /// list the IDL declares.
    ///
    /// The predecessor read the first 121 bytes by hand and stopped, so nine
    /// fields past that offset were invisible — including `ix_name`, which
    /// names the instruction that produced the event, and the executed
    /// `track_volume`, which is a different fact from the flag the caller
    /// requested.
    ///
    /// Two cross-checks ride along so this is a measurement rather than a
    /// transcription: the published rates must equal the two rate constants
    /// derived elsewhere in this crate, and each fee must equal its rate on
    /// the SOL leg rounded UP.
    #[test]
    fn a_real_trade_event_decodes_through_borsh() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            body_b64: String,
            expected: serde_json::Value,
        }
        let fx: Fixture =
            serde_json::from_str(include_str!("../../../fixtures/pumpfun/trade_event.json"))
                .expect("fixture parses");
        let body = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &fx.body_b64)
            .expect("body is base64");

        let ev = parse_trade_event_body(&body).expect("real body must decode");
        let want = &fx.expected;
        assert_eq!(ev.sol_amount, want["sol_amount"].as_u64().unwrap());
        assert_eq!(ev.fee, want["fee"].as_u64().unwrap());
        assert_eq!(ev.creator_fee, want["creator_fee"].as_u64().unwrap());
        assert_eq!(ev.fee_recipient.to_string(), want["fee_recipient"]);
        assert_eq!(ev.creator.to_string(), want["creator"]);

        use super::super::constants::{CREATOR_FEE_BPS, PROTOCOL_FEE_BPS};
        assert_eq!(ev.fee_basis_points, PROTOCOL_FEE_BPS);
        assert_eq!(ev.creator_fee_basis_points, CREATOR_FEE_BPS);
        let ceil = |amt: u64, bps: u64| (u128::from(amt) * u128::from(bps)).div_ceil(10_000) as u64;
        assert_eq!(ev.fee, ceil(ev.sol_amount, ev.fee_basis_points));
        assert_eq!(
            ev.creator_fee,
            ceil(ev.sol_amount, ev.creator_fee_basis_points)
        );

        // Past the old 121-byte cutoff: the event names its own instruction.
        assert!(!ev.ix_name.is_empty(), "ix_name was invisible before borsh");
    }

    /// Strict on trailing bytes. An event body is written by the program, so
    /// surplus bytes mean our layout is wrong, not that a sender appended junk.
    #[test]
    fn a_body_with_trailing_bytes_is_refused() {
        let fx: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/pumpfun/trade_event.json"))
                .unwrap();
        let mut body = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            fx["body_b64"].as_str().unwrap(),
        )
        .unwrap();
        assert!(parse_trade_event_body(&body).is_some());
        body.push(0);
        assert!(parse_trade_event_body(&body).is_none());
    }
}

#[cfg(test)]
mod v2_identity {
    use super::*;
    use crate::chain::NoContext;
    use crate::parsing::ParsedInstructionBuilder;
    use crate::protocols::pumpfun::accounts::derive_bonding_curve_pda;
    use crate::protocols::pumpfun::SELL_V2_DISCRIMINATOR;

    fn event_body_for(mint: Pubkey, user: Pubkey) -> Vec<u8> {
        borsh::to_vec(&TradeEvent {
            mint,
            user,
            sol_amount: 1_000,
            token_amount: 2_000,
            timestamp: 1_780_000_000,
            ..Default::default()
        })
        .expect("serialises")
    }

    fn v2_sell(accounts: Vec<Pubkey>, mint: Pubkey, user: Pubkey) -> Vec<ParsedInstruction> {
        let mut data = SELL_V2_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&[0u8; 16]);
        let outer = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(accounts)
            .data(data)
            .stack_height(1)
            .instruction_index(0)
            .build();

        let mut ev = ANCHOR_EVENT_TAG.to_vec();
        ev.extend_from_slice(&TRADE_EVENT_DISCRIMINATOR);
        ev.extend_from_slice(&event_body_for(mint, user));
        let mut inner = ParsedInstructionBuilder::new()
            .program_id(PUMPFUN_PROGRAM)
            .accounts(vec![])
            .data(ev)
            .stack_height(2)
            .instruction_index(1)
            .build();
        inner.parent_index = Some(0);
        vec![outer, inner]
    }

    /// v2 identity comes from the event and a PDA derivation, so it survives
    /// an account list of any length or order — which is the point, since the
    /// layout was observed at 26/27/28/29 slots.
    #[test]
    fn v2_identity_is_recovered_without_any_fixed_account_index() {
        let mint = Pubkey::new_unique();
        let user = Pubkey::new_unique();
        let pool = derive_bonding_curve_pda(&mint);

        for pad in [0usize, 5, 13] {
            let mut accounts = vec![Pubkey::new_unique(); pad];
            accounts.push(pool); // position deliberately varies
            accounts.extend(std::iter::repeat_with(Pubkey::new_unique).take(pad));

            let all = v2_sell(accounts, mint, user);
            let ev = PumpfunExtractor::extract(&all[0], &all, &NoContext)
                .unwrap_or_else(|| panic!("pad={pad} must extract"));
            match ev {
                ChainEvent::Swap(s) => {
                    assert_eq!(s.pool, pool);
                    assert_eq!(s.trader, user);
                    assert_eq!(s.instruction.name(), "sell_v2");
                }
                other => panic!("expected Swap, got {other:?}"),
            }
        }
    }

    /// If the derived curve is absent from the instruction's own accounts we
    /// have not corroborated the identity, so nothing is recorded. Without
    /// this a mint from an unrelated event would mint a plausible-looking row.
    #[test]
    fn an_uncorroborated_derivation_records_nothing() {
        let mint = Pubkey::new_unique();
        let user = Pubkey::new_unique();
        let all = v2_sell(vec![Pubkey::new_unique(); 26], mint, user);
        assert!(PumpfunExtractor::extract(&all[0], &all, &NoContext).is_none());
    }
}
