//! Assemble [`ParsedTransaction`]s from parsed instructions + tx metadata.
//!
//! This module owns the **protocol-agnostic** part of extraction:
//!
//! * [`TransactionHeader`] — the tx-level metadata the extractor can't
//!   derive from instructions alone (outcome, fees, compute, error
//!   strings).
//! * [`extract_transaction`] — the top-level entry point. Walks each
//!   instruction, dispatches through an [`ExtractorRegistry`], and
//!   assembles the envelope.
//! * [`classify_error`] — string-based `TxError` attribution.
//!
//! **Per-protocol adapters live inside their respective protocol
//! folders** — e.g. `protocols/pumpfun/extract.rs`. This mirrors
//! how `protocols/pumpfun/handler.rs` co-locates the storage
//! handler with the rest of the protocol code. Adding a new protocol:
//!
//! 1. Write `protocols/<protocol>/extract.rs` with a `FooExtractor`
//!    impl of [`ProtocolExtractor`].
//! 2. Expose it from `protocols/<protocol>/mod.rs`.
//! 3. Add `registry.register::<FooExtractor>()` in
//!    [`ExtractorRegistry::with_all_protocols`](registry::ExtractorRegistry::with_all_protocols).

mod context;
mod error;
mod registry;
mod traits;

pub use context::{ExtractContext, NoContext};
pub use error::{
    extract_failure_tally, extract_failures, report_extract_failure, ExtractError, Extracted,
};
pub use registry::{ExtractFn, ExtractorRegistry, ProtocolExtractor};
pub use traits::{
    child_event, corroborate, optional_child_event, ExtractsCreation, ExtractsCreatorFee, ExtractsLiquidity, ExtractsMigration, ExtractsSwap,
};

use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signature;

use crate::chain::types::{ParsedTransaction, TokenBalanceChange, TxError, TxOutcome};
use crate::parsing::ParsedInstruction;

// =============================================================================
// Header
// =============================================================================

/// Transaction-level metadata the extractor can't derive from
/// instructions. Populated by whichever adapter bridges the raw tx
/// source (Yellowstone gRPC update, RPC response, cached tx) to the
/// extractor.
#[derive(Debug, Clone)]
pub struct TransactionHeader {
    pub signature: Signature,
    pub slot: u64,
    /// Position within the slot (`TransactionUpdate.index`). Carried through to
    /// [`ParsedTransaction::index`]; see its doc for why it is captured here.
    pub index: u64,
    pub block_time: Option<i64>,
    /// `true` = confirmed success, `false` = tx-level error in meta.
    pub success: bool,
    pub fee_paid_lamports: u64,
    pub compute_used: Option<u64>,
    /// Raw error string from `meta.err` when `success == false`. Used
    /// for [`TxError`] classification — we don't require a typed
    /// representation since most of what we care about is in the
    /// string ("slippage", "insufficient funds") or the
    /// `InstructionError(index, custom_code)` shape below.
    pub error_message: Option<String>,
    /// Program + custom code from a failed instruction, if extractable
    /// from meta. Populated by the adapter; `None` when not available.
    /// Used for [`TxError::Rejected`].
    pub failed_program: Option<(Pubkey, u32)>,
    /// Paired pre→post token balances from the tx meta (see
    /// [`TokenBalanceChange::pair`]). Empty when the source carried no
    /// meta balances. Carried through to
    /// [`ParsedTransaction::token_balances`] for successful txs.
    pub token_balances: Vec<TokenBalanceChange>,
}

// =============================================================================
// Entry point
// =============================================================================

/// Assemble a [`ParsedTransaction`] from a header + parsed instructions.
///
/// Never fails — unrecognised programs are silently skipped, malformed
/// protocol events are logged at `warn` and dropped, and failed txs
/// produce no events (the state changes they'd have carried didn't
/// occur).
pub fn extract_transaction(
    header: TransactionHeader,
    instructions: &[ParsedInstruction],
    ctx: &dyn ExtractContext,
    registry: &ExtractorRegistry,
) -> ParsedTransaction {
    let outcome = if header.success {
        TxOutcome::Success
    } else {
        TxOutcome::Failed(classify_error(&header))
    };

    let (events, kept_instructions) = if header.success {
        let mut v = Vec::new();
        for ix in instructions {
            if let Some(event) = registry.extract(ix, instructions, ctx) {
                v.push(event);
            }
        }
        (v, instructions.to_vec())
    } else {
        // Failed tx: no state transitions, so no events and no
        // instructions to retain.
        (Vec::new(), Vec::new())
    };

    // Failed tx: like events/instructions, balances describe state
    // transitions that never occurred (pre == post), so drop them.
    let token_balances = if header.success {
        header.token_balances
    } else {
        Vec::new()
    };

    ParsedTransaction {
        signature: header.signature,
        slot: header.slot,
        index: header.index,
        block_time: header.block_time,
        fee_paid_lamports: header.fee_paid_lamports,
        compute_used: header.compute_used,
        outcome,
        events,
        instructions: kept_instructions,
        token_balances,
    }
}

// =============================================================================
// Error classification
// =============================================================================

fn classify_error(header: &TransactionHeader) -> TxError {
    let msg_lower = header.error_message.as_deref().map(str::to_lowercase);

    // Slippage detection: matches the common error strings across
    // Pumpfun / PumpSwap / Raydium. Cheap string contains — typed
    // per-protocol error enums are a later optimisation, and go behind
    // `Rejected { program_id, custom_code }` if a consumer demands
    // them.
    if let Some(msg) = &msg_lower {
        if msg.contains("slippage")
            || msg.contains("toolittlesolreceived")
            || msg.contains("toomuchsolrequired")
            || msg.contains("exceededslippage")
            || msg.contains("buyslippagebelowmintokensout")
        {
            return TxError::Slippage;
        }
        if msg.contains("insufficient funds") || msg.contains("insufficientfunds") {
            return TxError::InsufficientFunds;
        }
    }

    if let Some((program_id, custom_code)) = header.failed_program {
        return TxError::Rejected {
            program_id,
            custom_code,
        };
    }

    TxError::Other(header.error_message.clone().unwrap_or_default())
}

// =============================================================================
// Tests — orchestration only. Per-protocol tests live in their own modules.
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::ParsedInstructionBuilder;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    fn header(success: bool, error: Option<&str>) -> TransactionHeader {
        TransactionHeader {
            signature: Signature::default(),
            slot: 100,
            index: 0,
            block_time: Some(1_700_000_000),
            success,
            fee_paid_lamports: 5_000,
            compute_used: Some(50_000),
            error_message: error.map(str::to_string),
            failed_program: None,
            token_balances: Vec::new(),
        }
    }

    #[test]
    fn failed_tx_drops_token_balances() {
        let mut h = header(false, Some("some error"));
        h.token_balances = vec![TokenBalanceChange {
            owner: pk(0xA),
            mint: pk(0xB),
            program: pk(0xEE),
            pre_raw: Some(1),
            post_raw: Some(1),
            decimals: 6,
        }];
        let registry = ExtractorRegistry::with_all_protocols();
        let tx = extract_transaction(h, &[], &NoContext, &registry);
        assert!(tx.token_balances.is_empty());
    }

    #[test]
    fn successful_tx_carries_token_balances_through() {
        let mut h = header(true, None);
        h.token_balances = vec![TokenBalanceChange {
            owner: pk(0xA),
            mint: pk(0xB),
            program: pk(0xEE),
            pre_raw: None,
            post_raw: Some(9),
            decimals: 6,
        }];
        let registry = ExtractorRegistry::with_all_protocols();
        let tx = extract_transaction(h, &[], &NoContext, &registry);
        assert_eq!(tx.token_balances.len(), 1);
        assert_eq!(tx.token_balances[0].post_raw, Some(9));
    }

    #[test]
    fn slippage_error_string_classifies_as_slippage() {
        let tx = extract_transaction(
            header(false, Some("custom program error: TooMuchSolRequired")),
            &[],
            &NoContext,
            &ExtractorRegistry::with_all_protocols(),
        );
        assert!(matches!(tx.outcome, TxOutcome::Failed(TxError::Slippage)));
        assert!(tx.events.is_empty());
    }

    #[test]
    fn exceededslippage_substring_classifies_as_slippage() {
        let tx = extract_transaction(
            header(false, Some("InstructionError: ExceededSlippage")),
            &[],
            &NoContext,
            &ExtractorRegistry::with_all_protocols(),
        );
        assert!(matches!(tx.outcome, TxOutcome::Failed(TxError::Slippage)));
    }

    #[test]
    fn program_code_without_slippage_match_falls_back_to_rejected() {
        let mut h = header(false, Some("generic failure"));
        h.failed_program = Some((pk(0x99), 6004));
        let tx = extract_transaction(h, &[], &NoContext, &ExtractorRegistry::with_all_protocols());
        match tx.outcome {
            TxOutcome::Failed(TxError::Rejected {
                program_id,
                custom_code,
            }) => {
                assert_eq!(program_id, pk(0x99));
                assert_eq!(custom_code, 6004);
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn unattributed_error_becomes_other() {
        let tx = extract_transaction(
            header(false, Some("something weird")),
            &[],
            &NoContext,
            &ExtractorRegistry::with_all_protocols(),
        );
        match tx.outcome {
            TxOutcome::Failed(TxError::Other(msg)) => assert_eq!(msg, "something weird"),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn unknown_program_instructions_are_skipped_silently() {
        let ix = ParsedInstructionBuilder::new()
            .program_id(pk(0xFF))
            .accounts(vec![])
            .data(vec![0, 1, 2, 3, 4, 5, 6, 7, 8])
            .stack_height(1)
            .instruction_index(0)
            .build();

        let tx = extract_transaction(
            header(true, None),
            std::slice::from_ref(&ix),
            &NoContext,
            &ExtractorRegistry::with_all_protocols(),
        );
        assert!(tx.events.is_empty());
        assert!(tx.succeeded());
    }

    #[test]
    fn failed_tx_produces_no_events_even_with_valid_instructions() {
        // Even if instructions look extractable, a failed tx's state
        // never landed — the event vec must be empty.
        let ix = ParsedInstructionBuilder::new()
            .program_id(crate::protocols::pumpfun::PROGRAM_ID)
            .accounts(vec![])
            .data(vec![])
            .stack_height(1)
            .instruction_index(0)
            .build();
        let tx = extract_transaction(
            header(false, Some("slippage")),
            std::slice::from_ref(&ix),
            &NoContext,
            &ExtractorRegistry::with_all_protocols(),
        );
        assert!(tx.events.is_empty());
    }
}
