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

use super::events::{CollectCreatorFeeEvent, DistributeCreatorFeesEvent, TradeEvent};
use super::{
    BuyAccounts, BuyExactQuoteInV2Params, BuyExactSolInParams, BuyParams, BuyV2Params,
    CollectCreatorFeeParams, CollectCreatorFeeV2Params, CreateAccounts, CreateParams,
    CreateV2Accounts, CreateV2Params, DistributeCreatorFeesParams, DistributeCreatorFeesV2Params,
    PumpfunInstruction, SellAccounts, SellParams, SellV2Params, PROGRAM_ID as PUMPFUN_PROGRAM,
};
use crate::chain::{
    child_event, corroborate, report_extract_failure, ChainEvent, CreatorFee, CreatorPayout,
    CurveState, ExtractContext, ExtractError, Extracted, ExtractsCreation, ExtractsCreatorFee,
    ExtractsSwap, ProtocolExtractor, Swap, TokenCreation,
};
use crate::parsing::anchor::ANCHOR_EVENT_TAG;
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
        ctx: &dyn ExtractContext,
    ) -> Option<ChainEvent> {
        match Self::try_extract(ix, all_instructions, ctx) {
            Ok(event) => event,
            Err(e) => {
                // Reported, never swallowed. The caller has the signature and
                // turns this into a counted, retrievable sample; here we only
                // have the instruction index, which is why this used to be an
                // unactionable `warn!` nobody could reach.
                report_extract_failure(&Protocol::Pumpfun, ix, &e);
                None
            }
        }
    }
}

impl PumpfunExtractor {
    /// The whole extractor, as a `Result`.
    ///
    /// `Ok(None)` is routine — an inner event self-CPI, or an instruction that
    /// declares no event. `Err` is a gap in us: an instruction we recognise that
    /// should have produced something and did not.
    fn try_extract(
        ix: &ParsedInstruction,
        all: &[ParsedInstruction],
        ctx: &dyn ExtractContext,
    ) -> Extracted {
        // Anchor emits events as self-CPIs on the same program, so an event ix
        // reaches this extractor looking like an instruction. The parent already
        // produced the event; skipping keys on the *tag*, which every Anchor
        // event carries, not on one event's discriminator.
        if ix.data.len() >= 8 && ix.data[..8] == ANCHOR_EVENT_TAG {
            return Ok(None);
        }

        let parsed = match PumpfunInstruction::try_from_slice(&ix.data) {
            Ok(v) => v,
            Err(e) => {
                // An instruction on a program we claim to decode. Retained with
                // its bytes so the decoder can be fixed against real data.
                crate::undecoded::report(&ix.program_id, &ix.data, &ix.accounts, &format!("{e:?}"));
                return Ok(None);
            }
        };

        // One arm per variant, each delegating to the trait that variant
        // implements. Nothing here decides *how* an event is built — only which
        // question to ask — so a new instruction is a new arm plus an impl, and
        // an instruction that declares no event cannot silently fall through.
        match &parsed {
            PumpfunInstruction::Buy(p) => swap_via(p, ix, all, ctx),
            PumpfunInstruction::BuyExactSolIn(p) => swap_via(p, ix, all, ctx),
            PumpfunInstruction::Sell(p) => swap_via(p, ix, all, ctx),
            PumpfunInstruction::BuyV2(p) => swap_via(p, ix, all, ctx),
            PumpfunInstruction::BuyExactQuoteInV2(p) => swap_via(p, ix, all, ctx),
            PumpfunInstruction::SellV2(p) => swap_via(p, ix, all, ctx),
            PumpfunInstruction::Create(p) => {
                Ok(Some(ChainEvent::TokenCreation(p.creation(ix, ctx)?)))
            }
            PumpfunInstruction::CreateV2(p) => {
                Ok(Some(ChainEvent::TokenCreation(p.creation(ix, ctx)?)))
            }
            PumpfunInstruction::CollectCreatorFee(p) => fee_via(p, ix, all),
            PumpfunInstruction::CollectCreatorFeeV2(p) => fee_via(p, ix, all),
            PumpfunInstruction::DistributeCreatorFees(p) => fee_via(p, ix, all),
            PumpfunInstruction::DistributeCreatorFeesV2(p) => fee_via(p, ix, all),
        }
    }
}

/// Resolve an instruction's event, then let it build its own swap.
fn swap_via<T: ExtractsSwap>(
    params: &T,
    ix: &ParsedInstruction,
    all: &[ParsedInstruction],
    ctx: &dyn ExtractContext,
) -> Extracted {
    let event = child_event::<T::Event>(ix, all, &PUMPFUN_PROGRAM)?;
    Ok(Some(ChainEvent::Swap(params.swap(&event, ix, ctx)?)))
}

/// Same, for the creator-fee family.
fn fee_via<T: ExtractsCreatorFee>(
    params: &T,
    ix: &ParsedInstruction,
    all: &[ParsedInstruction],
) -> Extracted {
    let event = child_event::<T::Event>(ix, all, &PUMPFUN_PROGRAM)?;
    Ok(Some(ChainEvent::CreatorFee(
        params.creator_fee(&event, ix)?,
    )))
}

// ─────────────────────────────────────────────────────────────────────────────
// Swaps
// ─────────────────────────────────────────────────────────────────────────────

/// Everything a pumpfun swap needs beyond the event, resolved per instruction.
struct SwapIdentity {
    mint: Pubkey,
    pool: Pubkey,
    trader: Pubkey,
}

/// The v1 forms read identity from fixed account slots.
fn identity_from_v1_buy(ix: &ParsedInstruction) -> Result<SwapIdentity, ExtractError> {
    let a = BuyAccounts::from_account_keys(&ix.accounts).map_err(|source| {
        ExtractError::AccountLayout {
            expected: "BuyAccounts",
            source,
        }
    })?;
    Ok(SwapIdentity {
        mint: a.mint,
        pool: a.bonding_curve,
        trader: a.user,
    })
}

fn identity_from_v1_sell(ix: &ParsedInstruction) -> Result<SwapIdentity, ExtractError> {
    let a = SellAccounts::from_account_keys(&ix.accounts).map_err(|source| {
        ExtractError::AccountLayout {
            expected: "SellAccounts",
            source,
        }
    })?;
    Ok(SwapIdentity {
        mint: a.mint,
        pool: a.bonding_curve,
        trader: a.user,
    })
}

/// The v2 forms carry a *variable* account list — 26/27/28/29 slots observed on
/// mainnet — so no fixed index is safe and reading them through a v1 struct
/// would yield the wrong mint, pool and trader while looking successful.
///
/// They do not need one. The event names the mint and the trader, and the
/// bonding curve is a PDA *of* the mint, so identity is recovered without
/// reference to any slot. The derived PDA is then corroborated against the
/// instruction's own account list: a swap really did touch its curve, so a
/// derivation that does not appear there is one we cannot stand behind.
fn identity_from_v2(ix: &ParsedInstruction, ev: &TradeEvent) -> Result<SwapIdentity, ExtractError> {
    let pool = super::accounts::derive_bonding_curve_pda(&ev.mint);
    if !ix.accounts.contains(&pool) {
        return Err(ExtractError::Corroboration {
            field: "bonding_curve derived from the event mint",
            from_event: pool.to_string(),
            from_accounts: "absent from the instruction's accounts".to_string(),
        });
    }
    Ok(SwapIdentity {
        mint: ev.mint,
        pool,
        trader: ev.user,
    })
}

/// Map a `TradeEvent` onto the token-agnostic [`Swap`], given identity.
///
/// Amounts come from the event, never the arguments: the declared side is a
/// slippage bound, not a fill. Pumpfun curves are SOL-denominated on one side,
/// so direction decides which token each amount belongs to.
fn swap_from(
    id: &SwapIdentity,
    ev: &TradeEvent,
    ix: &ParsedInstruction,
    track_volume: crate::protocols::OptionBool,
) -> Swap {
    let (token_in, amount_in, token_out, amount_out, reserve_in, reserve_out) = if ev.is_buy {
        (
            crate::tokens::WSOL,
            ev.sol_amount,
            id.mint,
            ev.token_amount,
            ev.virtual_sol_reserves,
            ev.virtual_token_reserves,
        )
    } else {
        (
            id.mint,
            ev.token_amount,
            crate::tokens::WSOL,
            ev.sol_amount,
            ev.virtual_token_reserves,
            ev.virtual_sol_reserves,
        )
    };

    Swap {
        track_volume,
        instruction: crate::swap_instruction::resolve(&ix.program_id, &ix.data),
        protocol: Protocol::Pumpfun,
        pool: id.pool,
        trader: id.trader,
        token_in,
        amount_in,
        token_out,
        amount_out,
        // Read off the event, not recomputed: the chain publishes the exact
        // protocol + creator lamports it charged. Pumpfun charges in SOL
        // regardless of direction.
        fee_amount: ev.fee.saturating_add(ev.creator_fee),
        fee_mint: crate::tokens::WSOL,
        state_before: None,
        state_after: Some(CurveState::Reserves {
            in_side: reserve_in,
            out_side: reserve_out,
        }),
    }
}

impl ExtractsSwap for BuyParams {
    type Event = TradeEvent;

    fn swap(
        &self,
        event: &TradeEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        let id = identity_from_v1_buy(ix)?;
        // Two independent sources of the same fact. They used to be compared
        // and the mismatch merely logged, so a disagreement still reached the
        // tape; it now refuses, matching every other corroboration check.
        corroborate("mint", &event.mint, &id.mint)?;
        Ok(swap_from(&id, event, ix, self.track_volume))
    }
}

impl ExtractsSwap for BuyExactSolInParams {
    type Event = TradeEvent;

    fn swap(
        &self,
        event: &TradeEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        // Shares `buy`'s 16-slot account layout exactly; only the discriminator
        // and the pinned side differ, and which instruction ran is preserved on
        // the row so the two are never graded as one.
        let id = identity_from_v1_buy(ix)?;
        corroborate("mint", &event.mint, &id.mint)?;
        Ok(swap_from(&id, event, ix, self.track_volume))
    }
}

impl ExtractsSwap for SellParams {
    type Event = TradeEvent;

    fn swap(
        &self,
        event: &TradeEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        let id = identity_from_v1_sell(ix)?;
        corroborate("mint", &event.mint, &id.mint)?;
        Ok(swap_from(
            &id,
            event,
            ix,
            crate::protocols::OptionBool::None,
        ))
    }
}

impl ExtractsSwap for BuyV2Params {
    type Event = TradeEvent;

    fn swap(
        &self,
        event: &TradeEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        let id = identity_from_v2(ix, event)?;
        // No `track_volume`: the IDL declares none for this instruction and 0 of
        // 208 mainnet instructions carried one, unlike its v2 sibling.
        Ok(swap_from(
            &id,
            event,
            ix,
            crate::protocols::OptionBool::None,
        ))
    }
}

impl ExtractsSwap for BuyExactQuoteInV2Params {
    type Event = TradeEvent;

    fn swap(
        &self,
        event: &TradeEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        let id = identity_from_v2(ix, event)?;
        // Carried even though no IDL declares it — measured on 150 of 621
        // mainnet instructions, and the only record that the trade opted in.
        Ok(swap_from(&id, event, ix, self.track_volume))
    }
}

impl ExtractsSwap for SellV2Params {
    type Event = TradeEvent;

    fn swap(
        &self,
        event: &TradeEvent,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError> {
        let id = identity_from_v2(ix, event)?;
        Ok(swap_from(
            &id,
            event,
            ix,
            crate::protocols::OptionBool::None,
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Creations
// ─────────────────────────────────────────────────────────────────────────────

impl ExtractsCreation for CreateParams {
    fn creation(
        &self,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<TokenCreation, ExtractError> {
        let a = CreateAccounts::from_account_keys(&ix.accounts).map_err(|source| {
            ExtractError::AccountLayout {
                expected: "CreateAccounts",
                source,
            }
        })?;
        Ok(TokenCreation {
            protocol: Protocol::Pumpfun,
            mint: a.mint,
            pool: a.bonding_curve,
            creator: a.user,
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            uri: self.uri.clone(),
        })
    }
}

impl ExtractsCreation for CreateV2Params {
    fn creation(
        &self,
        ix: &ParsedInstruction,
        _ctx: &dyn ExtractContext,
    ) -> Result<TokenCreation, ExtractError> {
        let a = CreateV2Accounts::from_account_keys(&ix.accounts).map_err(|source| {
            ExtractError::AccountLayout {
                expected: "CreateV2Accounts",
                source,
            }
        })?;
        // Prefer the explicit `creator` argument over the signer where it is
        // populated — it is what pumpfun stores as canonical. They almost always
        // agree; the rare divergence is a launch service signing for an end user.
        let creator = if self.creator == Pubkey::default() {
            a.user
        } else {
            self.creator
        };
        Ok(TokenCreation {
            protocol: Protocol::Pumpfun,
            mint: a.mint,
            pool: a.bonding_curve,
            creator,
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            uri: self.uri.clone(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Creator fees
// ─────────────────────────────────────────────────────────────────────────────

/// A creator draining their own vault. No mint: the vault accrues across every
/// token that creator launched, so the chain makes no attribution to one.
fn collected(ev: &CollectCreatorFeeEvent) -> CreatorFee {
    CreatorFee {
        protocol: Protocol::Pumpfun,
        payout: CreatorPayout::Direct {
            creator: ev.creator,
        },
        amount: ev.creator_fee,
        quote_mint: ev.quote_mint,
        mint: None,
        timestamp: ev.timestamp,
    }
}

impl ExtractsCreatorFee for CollectCreatorFeeParams {
    type Event = CollectCreatorFeeEvent;

    fn creator_fee(
        &self,
        event: &CollectCreatorFeeEvent,
        _ix: &ParsedInstruction,
    ) -> Result<CreatorFee, ExtractError> {
        Ok(collected(event))
    }
}

impl ExtractsCreatorFee for CollectCreatorFeeV2Params {
    type Event = CollectCreatorFeeEvent;

    fn creator_fee(
        &self,
        event: &CollectCreatorFeeEvent,
        _ix: &ParsedInstruction,
    ) -> Result<CreatorFee, ExtractError> {
        // Settles into a token account rather than a bare lamport transfer; the
        // event, and so the recorded fact, is identical.
        Ok(collected(event))
    }
}

/// A vault split across a sharing config. Unlike a collect this one *is*
/// attributable: the event names the mint whose trading earned the fees.
fn distributed(ev: &DistributeCreatorFeesEvent) -> CreatorFee {
    CreatorFee {
        protocol: Protocol::Pumpfun,
        payout: CreatorPayout::Shared {
            bonding_curve: ev.bonding_curve,
            sharing_config: ev.sharing_config,
            admin: ev.admin,
            shareholders: ev.shareholders.clone(),
        },
        amount: ev.distributed,
        quote_mint: ev.quote_mint,
        mint: Some(ev.mint),
        timestamp: ev.timestamp,
    }
}

impl ExtractsCreatorFee for DistributeCreatorFeesParams {
    type Event = DistributeCreatorFeesEvent;

    fn creator_fee(
        &self,
        event: &DistributeCreatorFeesEvent,
        _ix: &ParsedInstruction,
    ) -> Result<CreatorFee, ExtractError> {
        Ok(distributed(event))
    }
}

impl ExtractsCreatorFee for DistributeCreatorFeesV2Params {
    type Event = DistributeCreatorFeesEvent;

    fn creator_fee(
        &self,
        event: &DistributeCreatorFeesEvent,
        _ix: &ParsedInstruction,
    ) -> Result<CreatorFee, ExtractError> {
        Ok(distributed(event))
    }
}

// =============================================================================
// Tests — Pumpfun-specific only. Cross-protocol orchestration tests live
// in `chain/extract/mod.rs`.
// =============================================================================

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::super::events::TRADE_EVENT_DISCRIMINATOR;
    use super::*;
    use crate::chain::NoContext;
    #[allow(unused_imports)]
    use crate::parsing::event::ProtocolEvent;
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
    /// Matching on the Anchor tag alone rather than the event's own
    /// discriminator fails this test.
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
    #[allow(unused_imports)]
    use super::super::events::TRADE_EVENT_DISCRIMINATOR;
    use super::*;
    #[allow(unused_imports)]
    use crate::parsing::event::ProtocolEvent;

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

        let ev = TradeEvent::from_event_body(&body).expect("real body must decode");
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
        assert!(TradeEvent::from_event_body(&body).is_ok());
        body.push(0);
        assert!(
            TradeEvent::from_event_body(&body).is_err(),
            "trailing bytes must be refused, not ignored"
        );
    }
}

#[cfg(test)]
mod v2_identity {
    #[allow(unused_imports)]
    use super::super::events::TRADE_EVENT_DISCRIMINATOR;
    use super::*;
    use crate::chain::NoContext;
    #[allow(unused_imports)]
    use crate::parsing::event::ProtocolEvent;
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
