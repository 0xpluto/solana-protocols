//! Semantic chain events + the extractor that produces them.
//!
//! See `types` for the data shape and `extract` for the
//! extractor orchestration. Per-protocol extractor adapters live
//! in their respective `protocols/<name>/extract.rs` file and are
//! registered into [`ExtractorRegistry`].
//!
//! ```text
//! raw tx → parsing::parse_instructions → Vec<ParsedInstruction>
//!        → chain::extract_transaction  → ParsedTransaction
//! ```

mod extract;
mod types;

pub use extract::{
    child_event, corroborate, extract_failure_tally, extract_failures, extract_transaction,
    report_extract_failure, ExtractContext, ExtractError, ExtractFn, Extracted, ExtractorRegistry,
    ExtractsCreation, ExtractsCreatorFee, ExtractsMigration, ExtractsSwap, NoContext,
    ProtocolExtractor, TransactionHeader,
};
pub use types::{
    ChainEvent, CreatorFee, CreatorPayout, CurveState, Migration, ParsedTransaction, Swap,
    TokenBalanceChange, TokenBalanceEntry, TokenCreation, TxError, TxOutcome,
};
