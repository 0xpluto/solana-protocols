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
    extract_transaction, ExtractContext, ExtractFn, ExtractorRegistry, NoContext,
    ProtocolExtractor, TransactionHeader,
};
pub use types::{
    ChainEvent, CreatorFee, CreatorPayout, CurveState, Migration, ParsedTransaction, Swap,
    TokenBalanceChange, TokenBalanceEntry, TokenCreation, TxError, TxOutcome,
};
