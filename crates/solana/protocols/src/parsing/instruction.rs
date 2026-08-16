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

    for entry in entries {
        match frame_effect(&entry) {
            FrameEffect::Cut => {
                if let Some(&top) = frames.last() {
                    result[top].logs.push(entry);
                }
                for ix in result.iter_mut().skip(cursor) {
                    ix.logs.push(LogEntry::Truncated);
                }
                return;
            }
            FrameEffect::Open(program, depth) => {
                let Some(expected) = result.get(cursor) else {
                    tracing::warn!(
                        %program,
                        depth,
                        "log stream invokes past the last instruction; logs left unattached"
                    );
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
