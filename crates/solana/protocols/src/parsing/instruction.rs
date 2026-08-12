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

/// Parse a flat list of instructions with log association.
///
/// This implements the stack-based algorithm from the legacy code but
/// produces a flat vector with parent indices instead of a tree.
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
    I: Iterator<Item = (u8, &'a [u8], Vec<u8>, Option<u32>)>, // (program_id_idx, account_indices, data, stack_height)
    L: Iterator<Item = &'a str>,
{
    use super::log::parse_logs;

    let mut result: Vec<ParsedInstruction> = Vec::new();
    let mut log_entries: Vec<LogEntry> = parse_logs(logs);
    let mut log_iter = log_entries.drain(..).peekable();

    // Stack tracks (instruction_index, stack_height)
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

        // Pop stack until we find parent
        while let Some(&(_, parent_height)) = stack.last() {
            if parent_height >= stack_height {
                // Collect logs for the popped instruction
                let (popped_idx, _) = stack.pop().unwrap();
                collect_logs_until_invoke_or_success(&mut log_iter, &mut result[popped_idx].logs);
            } else {
                break;
            }
        }

        // Determine parent
        let parent_index = stack.last().map(|&(idx, _)| idx);

        // Create instruction
        let mut instruction = ParsedInstruction::new(program_id, accounts, data, stack_height, idx);
        instruction.parent_index = parent_index;

        // Collect initial logs for this instruction
        collect_logs_until_invoke_or_success(&mut log_iter, &mut instruction.logs);

        let current_idx = result.len();
        result.push(instruction);
        stack.push((current_idx, stack_height));
    }

    // Drain remaining stack
    while let Some((popped_idx, _)) = stack.pop() {
        collect_logs_until_invoke_or_success(&mut log_iter, &mut result[popped_idx].logs);
    }

    result
}

/// Collect logs until we see an Invoke or Success log.
fn collect_logs_until_invoke_or_success<I>(
    logs: &mut std::iter::Peekable<I>,
    target: &mut Vec<LogEntry>,
) where
    I: Iterator<Item = LogEntry>,
{
    let first_was_invoke = logs
        .peek()
        .is_some_and(|l| matches!(l, LogEntry::Invoke { .. }));
    let mut count = 0;

    while let Some(log) = logs.peek() {
        let is_invoke = matches!(log, LogEntry::Invoke { .. });
        let is_success = matches!(log, LogEntry::Success { .. });

        // Stop if we hit another invoke (after first) or success (if first was invoke)
        if (count > 0 && is_invoke) || (first_was_invoke && is_success) {
            break;
        }

        let log = logs.next().unwrap();
        let should_stop = matches!(log, LogEntry::Success { .. });
        target.push(log);

        if should_stop {
            break;
        }
        count += 1;
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
