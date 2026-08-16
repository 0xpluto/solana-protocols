//! Parsed instruction types.
//!
//! Provides [`ParsedInstruction`] - a simplified, flat representation of
//! Solana instructions with resolved accounts and associated logs.
//!
//! Unlike the legacy `StructuredInstruction` which uses `Rc<RefCell<>>` for
//! tree structure, this uses a flat vector with parent indices. Benefits:
//! - No reference counting overhead
//! - Serializable (can cache parsed transactions)
//! - Simpler lifetime management
//! - Thread-safe (no RefCell)

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;

use super::log::LogEntry;

/// A parsed instruction with resolved accounts and associated logs.
///
/// This is a flat representation - parent/child relationships are tracked
/// via indices into the instruction vector, not nested references.
///
/// # Fields
///
/// - `program_id`: The program that will execute this instruction
/// - `accounts`: Account pubkeys (already resolved from indices)
/// - `data`: Instruction data (already decoded from base58/base64)
/// - `logs`: Log entries associated with this instruction
/// - `stack_height`: CPI depth (1 = top-level, 2+ = inner)
/// - `parent_index`: Index of parent instruction (None for top-level)
/// - `instruction_index`: Position in flattened instruction list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedInstruction {
    /// Program ID that executes this instruction.
    pub program_id: Pubkey,

    /// Account pubkeys (resolved from indices).
    pub accounts: Vec<Pubkey>,

    /// Raw instruction data (decoded).
    pub data: Vec<u8>,

    /// Log entries associated with this instruction.
    pub logs: Vec<LogEntry>,

    /// CPI stack height (1 = top-level).
    pub stack_height: u32,

    /// Index of parent instruction in the same vector.
    /// None for top-level instructions.
    pub parent_index: Option<usize>,

    /// Index of this instruction in the flattened list.
    pub instruction_index: usize,
}

impl ParsedInstruction {
    /// Create a new parsed instruction.
    #[must_use]
    pub fn new(
        program_id: Pubkey,
        accounts: Vec<Pubkey>,
        data: Vec<u8>,
        stack_height: u32,
        instruction_index: usize,
    ) -> Self {
        Self {
            program_id,
            accounts,
            data,
            logs: Vec::new(),
            stack_height,
            parent_index: None,
            instruction_index,
        }
    }

    /// Check if this is a top-level instruction.
    #[must_use]
    pub fn is_top_level(&self) -> bool {
        self.stack_height == 1
    }

    /// Check if this is a CPI (inner) instruction.
    #[must_use]
    pub fn is_inner(&self) -> bool {
        self.stack_height > 1
    }

    /// Get the instruction discriminator (first 8 bytes of data).
    ///
    /// Returns None if data is less than 8 bytes.
    #[must_use]
    pub fn discriminator(&self) -> Option<[u8; 8]> {
        if self.data.len() >= 8 {
            let mut disc = [0u8; 8];
            disc.copy_from_slice(&self.data[..8]);
            Some(disc)
        } else {
            None
        }
    }

    /// Get instruction data after the discriminator.
    ///
    /// Returns empty slice if data is less than 8 bytes.
    #[must_use]
    pub fn data_after_discriminator(&self) -> &[u8] {
        if self.data.len() > 8 {
            &self.data[8..]
        } else {
            &[]
        }
    }

    /// Find a "Program data:" log and extract its decoded bytes.
    ///
    /// Returns the first Data log entry's payload.
    #[must_use]
    pub fn find_data_log(&self) -> Option<&[u8]> {
        self.logs.iter().find_map(|log| {
            if let LogEntry::Data { payload, .. } = log {
                Some(payload.as_slice())
            } else {
                None
            }
        })
    }

    /// Find a "Program data:" log with a specific discriminator.
    #[must_use]
    pub fn find_data_log_with_discriminator(&self, expected: &[u8; 8]) -> Option<&[u8]> {
        self.logs.iter().find_map(|log| {
            if let LogEntry::Data {
                discriminator,
                payload,
            } = log
            {
                if discriminator == expected {
                    Some(payload.as_slice())
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Check if logs were truncated (common with compute-heavy transactions).
    #[must_use]
    pub fn logs_truncated(&self) -> bool {
        self.logs
            .iter()
            .any(|log| matches!(log, LogEntry::Truncated))
    }

    /// Get the account at a specific index.
    #[must_use]
    pub fn account(&self, index: usize) -> Option<&Pubkey> {
        self.accounts.get(index)
    }

    /// Get the number of accounts.
    #[must_use]
    pub fn num_accounts(&self) -> usize {
        self.accounts.len()
    }
}

/// Builder for constructing ParsedInstruction.
#[derive(Debug, Default)]
pub struct ParsedInstructionBuilder {
    program_id: Option<Pubkey>,
    accounts: Vec<Pubkey>,
    data: Vec<u8>,
    logs: Vec<LogEntry>,
    stack_height: u32,
    parent_index: Option<usize>,
    instruction_index: usize,
}

impl ParsedInstructionBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stack_height: 1,
            ..Default::default()
        }
    }

    /// Set the program ID.
    #[must_use]
    pub fn program_id(mut self, program_id: Pubkey) -> Self {
        self.program_id = Some(program_id);
        self
    }

    /// Set the accounts.
    #[must_use]
    pub fn accounts(mut self, accounts: Vec<Pubkey>) -> Self {
        self.accounts = accounts;
        self
    }

    /// Set the instruction data.
    #[must_use]
    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    /// Set the logs.
    #[must_use]
    pub fn logs(mut self, logs: Vec<LogEntry>) -> Self {
        self.logs = logs;
        self
    }

    /// Add a log entry.
    #[must_use]
    pub fn log(mut self, log: LogEntry) -> Self {
        self.logs.push(log);
        self
    }

    /// Set the stack height.
    #[must_use]
    pub fn stack_height(mut self, height: u32) -> Self {
        self.stack_height = height;
        self
    }

    /// Set the parent index.
    #[must_use]
    pub fn parent_index(mut self, index: usize) -> Self {
        self.parent_index = Some(index);
        self
    }

    /// Set the instruction index.
    #[must_use]
    pub fn instruction_index(mut self, index: usize) -> Self {
        self.instruction_index = index;
        self
    }

    /// Build the ParsedInstruction.
    ///
    /// # Panics
    ///
    /// Panics if program_id was not set.
    #[must_use]
    pub fn build(self) -> ParsedInstruction {
        ParsedInstruction {
            program_id: self.program_id.expect("program_id is required"),
            accounts: self.accounts,
            data: self.data,
            logs: self.logs,
            stack_height: self.stack_height,
            parent_index: self.parent_index,
            instruction_index: self.instruction_index,
        }
    }
}

/// One instruction as the wire hands it over, before account indices are
/// resolved: `(program index, account indices, data, stack height)`.
///
/// `stack_height` is `None` on pre-v1.14 transactions, where the runtime did
/// not report CPI depth; [`parse_instructions`] reads that as top level.
pub type RawInstruction<'a> = (u8, &'a [u8], Vec<u8>, Option<u32>);

/// Parse a flat list of instructions with log association.
///
/// Produces a flat vector with parent indices instead of a tree, and hands each
/// instruction the log slice the runtime emitted for it.
///
/// # Arguments
///
/// * `instructions` - Iterator of (program_id, accounts, data, stack_height)
/// * `logs` - Iterator of log strings
/// * `account_keys` - Full list of account pubkeys for resolving indices
///
/// # Returns
///
/// Vector of parsed instructions with logs associated to the correct instruction.
pub fn parse_instructions<'a, I, L>(
    instructions: I,
    logs: L,
    account_keys: &[Pubkey],
) -> Vec<ParsedInstruction>
where
    I: Iterator<Item = RawInstruction<'a>>,
    L: Iterator<Item = &'a str>,
{
    use super::log::parse_logs;

    let mut result: Vec<ParsedInstruction> = Vec::new();
    // Stack of (instruction_index, stack_height) for parent resolution.
    let mut stack: Vec<(usize, u32)> = Vec::new();

    for (idx, (program_id_idx, account_indices, data, stack_height)) in instructions.enumerate() {
        let stack_height = stack_height.unwrap_or(1);
        let program_id = account_keys
            .get(program_id_idx as usize)
            .copied()
            .unwrap_or_default();

        let accounts: Vec<Pubkey> = account_indices
            .iter()
            .filter_map(|&i| account_keys.get(i as usize).copied())
            .collect();

        while stack.last().is_some_and(|&(_, h)| h >= stack_height) {
            stack.pop();
        }

        let mut instruction = ParsedInstruction::new(program_id, accounts, data, stack_height, idx);
        instruction.parent_index = stack.last().map(|&(i, _)| i);
        result.push(instruction);
        stack.push((idx, stack_height));
    }

    attach_logs(&mut result, parse_logs(logs));
    result
}

// ---------------------------------------------------------------------------
// Attribution health
// ---------------------------------------------------------------------------

static ATTR_TX: AtomicU64 = AtomicU64::new(0);
static ATTR_TX_COMPLETE: AtomicU64 = AtomicU64::new(0);
static ATTR_TX_TRUNCATED: AtomicU64 = AtomicU64::new(0);
static ATTR_TX_DESYNCED: AtomicU64 = AtomicU64::new(0);
static ATTR_TX_OVERRUN: AtomicU64 = AtomicU64::new(0);
static ATTR_IX: AtomicU64 = AtomicU64::new(0);
static ATTR_IX_OPENED: AtomicU64 = AtomicU64::new(0);

/// Running health of the log-attribution walk.
///
/// Counted unconditionally rather than behind a flag, for the same reason the
/// no-handler tally is: this walk fails *silently* — a mis-sliced instruction
/// still has a list of logs — so the only thing that can notice is a number
/// that was always being kept. Plain atomics, so the crate imposes no metrics
/// framework on consumers; the binary publishes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributionStats {
    /// Transactions whose logs were attributed.
    pub transactions: u64,
    /// Every instruction received its own `invoke`.
    pub complete: u64,
    /// The runtime cut the log stream; instructions past the cut are marked.
    pub truncated: u64,
    /// An `invoke` named a program the instruction list did not expect.
    pub desynced: u64,
    /// The stream invoked past the last instruction.
    pub overrun: u64,
    /// Instructions seen.
    pub instructions: u64,
    /// Instructions that got their own opening `invoke`.
    pub instructions_opened: u64,
}

impl AttributionStats {
    /// Fraction of instructions handed their own log slice, `None` before any
    /// instruction has been seen — a ratio over nothing is not 100%.
    #[must_use]
    pub fn instruction_coverage(&self) -> Option<f64> {
        (self.instructions > 0).then(|| self.instructions_opened as f64 / self.instructions as f64)
    }
}

/// Snapshot the attribution counters.
#[must_use]
pub fn attribution_stats() -> AttributionStats {
    AttributionStats {
        transactions: ATTR_TX.load(Ordering::Relaxed),
        complete: ATTR_TX_COMPLETE.load(Ordering::Relaxed),
        truncated: ATTR_TX_TRUNCATED.load(Ordering::Relaxed),
        desynced: ATTR_TX_DESYNCED.load(Ordering::Relaxed),
        overrun: ATTR_TX_OVERRUN.load(Ordering::Relaxed),
        instructions: ATTR_IX.load(Ordering::Relaxed),
        instructions_opened: ATTR_IX_OPENED.load(Ordering::Relaxed),
    }
}

/// What a log entry does to the invocation stack.
///
/// Extracted before the entry is moved into its owner, and exhaustive so a new
/// [`LogEntry`] variant has to declare its structural role rather than
/// defaulting into the body of whatever frame happens to be open.
enum FrameEffect {
    /// Opens a new frame for `(program, depth)`.
    Open(Pubkey, u32),
    /// Closes the innermost open frame.
    Close,
    /// The runtime cut the stream; nothing after it is attributable.
    Cut,
    /// Belongs to the innermost open frame.
    Body,
}

fn frame_effect(entry: &LogEntry) -> FrameEffect {
    match entry {
        LogEntry::Invoke { program, depth } => FrameEffect::Open(*program, *depth),
        LogEntry::Success { .. } | LogEntry::Failed { .. } => FrameEffect::Close,
        LogEntry::Truncated => FrameEffect::Cut,
        LogEntry::Data { .. }
        | LogEntry::Return { .. }
        | LogEntry::Message { .. }
        | LogEntry::Consumed { .. }
        | LogEntry::Unknown { .. } => FrameEffect::Body,
    }
}

/// Give every instruction the log slice the runtime emitted for it.
///
/// The stream's `invoke` lines are the authority. They appear in execution
/// order, which is the order the flattened instruction list is built in, so the
/// nth `invoke` opens the nth instruction — and that correspondence is
/// **verified** against the instruction's own program and stack height rather
/// than assumed. Everything between an open and its terminator belongs to that
/// frame, except regions owned by deeper frames: a child's logs are the child's.
///
/// The previous implementation walked positionally without consulting the
/// program at all, which let one mis-step shift every later window; measured
/// against mainnet fixtures only 41% of instructions ended up with their own
/// logs. See `tests/log_attribution.rs`, which pins this.
///
/// Two things are deliberately *not* guessed through:
///
/// * **Truncation.** Every instruction still awaiting its `invoke` gets a
///   [`LogEntry::Truncated`] marker, because an empty slice reads as "this
///   instruction logged nothing" and that is a different claim.
/// * **Desync.** If an `invoke` names a program the instruction list does not
///   expect, attribution stops and the remainder is left unattached. Assigning
///   the rest anyway would produce confident nonsense.
fn attach_logs(result: &mut [ParsedInstruction], entries: Vec<LogEntry>) {
    // Instruction indices whose frame is open, innermost last.
    let mut frames: Vec<usize> = Vec::new();
    // Next instruction awaiting its `invoke`.
    let mut cursor = 0usize;

    ATTR_TX.fetch_add(1, Ordering::Relaxed);
    ATTR_IX.fetch_add(result.len() as u64, Ordering::Relaxed);
    // `cursor` at return *is* the number of instructions that got their own
    // opening invoke, on every exit path, so the accounting cannot drift from
    // the walk.
    let finish = |cursor: usize, outcome: &AtomicU64| {
        ATTR_IX_OPENED.fetch_add(cursor as u64, Ordering::Relaxed);
        outcome.fetch_add(1, Ordering::Relaxed);
    };

    for entry in entries {
        match frame_effect(&entry) {
            FrameEffect::Cut => {
                // *Every* open frame lost its tail, not just the innermost:
                // each one is still waiting for a `success` the runtime will
                // never emit. Marking only the top left the outer frames
                // reading as `Unterminated`, which blames this walk for the
                // runtime's truncation.
                for &open in &frames {
                    result[open].logs.push(LogEntry::Truncated);
                }
                for ix in result.iter_mut().skip(cursor) {
                    ix.logs.push(LogEntry::Truncated);
                }
                drop(entry);
                finish(cursor, &ATTR_TX_TRUNCATED);
                return;
            }
            FrameEffect::Open(program, depth) => {
                let Some(expected) = result.get(cursor) else {
                    tracing::warn!(
                        %program,
                        depth,
                        "log stream invokes past the last instruction; logs left unattached"
                    );
                    finish(cursor, &ATTR_TX_OVERRUN);
                    return;
                };
                if expected.program_id != program || expected.stack_height != depth {
                    tracing::warn!(
                        index = cursor,
                        expected_program = %expected.program_id,
                        expected_depth = expected.stack_height,
                        found_program = %program,
                        found_depth = depth,
                        "log stream desynced from the instruction list; logs left unattached"
                    );
                    finish(cursor, &ATTR_TX_DESYNCED);
                    return;
                }
                result[cursor].logs.push(entry);
                frames.push(cursor);
                cursor += 1;
            }
            FrameEffect::Close => {
                let Some(top) = frames.pop() else {
                    tracing::warn!("program terminator with no open frame; log dropped");
                    continue;
                };
                result[top].logs.push(entry);
            }
            FrameEffect::Body => {
                if let Some(&top) = frames.last() {
                    result[top].logs.push(entry);
                }
                // Otherwise the line precedes every invoke and no frame owns it.
            }
        }
    }
    finish(cursor, &ATTR_TX_COMPLETE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_instruction_discriminator() {
        let ix = ParsedInstruction::new(
            Pubkey::new_unique(),
            vec![],
            vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A],
            1,
            0,
        );

        let disc = ix.discriminator();
        assert_eq!(disc, Some([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]));
        assert_eq!(ix.data_after_discriminator(), &[0x09, 0x0A]);
    }

    #[test]
    fn parsed_instruction_short_data() {
        let ix = ParsedInstruction::new(Pubkey::new_unique(), vec![], vec![0x01, 0x02, 0x03], 1, 0);

        assert!(ix.discriminator().is_none());
        assert!(ix.data_after_discriminator().is_empty());
    }

    #[test]
    fn parsed_instruction_builder() {
        let program_id = Pubkey::new_unique();
        let account = Pubkey::new_unique();

        let ix = ParsedInstructionBuilder::new()
            .program_id(program_id)
            .accounts(vec![account])
            .data(vec![1, 2, 3])
            .stack_height(2)
            .parent_index(0)
            .instruction_index(1)
            .build();

        assert_eq!(ix.program_id, program_id);
        assert_eq!(ix.accounts, vec![account]);
        assert_eq!(ix.data, vec![1, 2, 3]);
        assert_eq!(ix.stack_height, 2);
        assert_eq!(ix.parent_index, Some(0));
        assert_eq!(ix.instruction_index, 1);
        assert!(ix.is_inner());
        assert!(!ix.is_top_level());
    }

    /// The counters are global and every other test in this binary also parses,
    /// so absolute deltas are unmeasurable here. What *is* measurable — and is
    /// the thing that would silently rot — is the accounting identity: every
    /// walk takes exactly one exit, and no walk can open more instructions than
    /// it was given.
    #[test]
    fn attribution_accounting_balances() {
        let keys = vec![Pubkey::new_unique(), Pubkey::new_unique()];
        let logs = [
            format!("Program {} invoke [1]", keys[0]),
            format!("Program {} invoke [2]", keys[1]),
            format!("Program {} success", keys[1]),
            format!("Program {} success", keys[0]),
        ];
        let ixs = vec![
            (0u8, [].as_slice(), Vec::new(), Some(1)),
            (1u8, [].as_slice(), Vec::new(), Some(2)),
        ];
        let parsed = parse_instructions(ixs.into_iter(), logs.iter().map(String::as_str), &keys);
        assert_eq!(parsed.len(), 2);

        let s = attribution_stats();
        assert_eq!(
            s.transactions,
            s.complete + s.truncated + s.desynced + s.overrun,
            "every walk must take exactly one exit"
        );
        assert!(
            s.instructions_opened <= s.instructions,
            "cannot open more instructions than were handed over"
        );
        assert!(s.instruction_coverage().is_some_and(|c| c <= 1.0));
    }

    #[test]
    fn coverage_over_nothing_is_unknown_not_perfect() {
        let empty = AttributionStats {
            transactions: 0,
            complete: 0,
            truncated: 0,
            desynced: 0,
            overrun: 0,
            instructions: 0,
            instructions_opened: 0,
        };
        assert_eq!(empty.instruction_coverage(), None);
    }

    /// A cut mid-stream orphans every frame on the stack, not just the one
    /// that was executing. Found by the audit itself: `unterminated` was
    /// non-zero on live traffic and every captured case was an outer frame of a
    /// truncated transaction.
    #[test]
    fn truncation_marks_every_open_frame() {
        let keys = vec![Pubkey::new_unique(), Pubkey::new_unique()];
        let logs = [
            format!("Program {} invoke [1]", keys[0]),
            format!("Program {} invoke [2]", keys[1]),
            "Log truncated".to_string(),
        ];
        let ixs = vec![
            (0u8, [].as_slice(), Vec::new(), Some(1)),
            (1u8, [].as_slice(), Vec::new(), Some(2)),
        ];
        let parsed = parse_instructions(ixs.into_iter(), logs.iter().map(String::as_str), &keys);

        for ix in &parsed {
            assert!(
                ix.logs_truncated(),
                "instruction {} was left unmarked by the cut",
                ix.instruction_index
            );
        }
    }

    #[test]
    fn find_data_log() {
        let mut ix = ParsedInstruction::new(Pubkey::new_unique(), vec![], vec![], 1, 0);

        ix.logs.push(LogEntry::Invoke {
            program: Pubkey::new_unique(),
            depth: 1,
        });
        ix.logs.push(LogEntry::Data {
            discriminator: [1, 2, 3, 4, 5, 6, 7, 8],
            payload: vec![0xAA, 0xBB],
        });
        ix.logs.push(LogEntry::Success {
            program: Pubkey::new_unique(),
        });

        let data = ix.find_data_log();
        assert_eq!(data, Some([0xAA, 0xBB].as_slice()));

        let data_with_disc = ix.find_data_log_with_discriminator(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(data_with_disc, Some([0xAA, 0xBB].as_slice()));

        let no_match = ix.find_data_log_with_discriminator(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(no_match.is_none());
    }
}
