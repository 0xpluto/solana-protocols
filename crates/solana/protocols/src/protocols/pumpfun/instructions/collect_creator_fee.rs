//! Pump.fun `collect_creator_fee`.
//!
//! A creator withdrawing accrued fees to their own account.
//!
//! Zero arguments, so the instruction data carries no economics — everything
//! worth recording is in the event it emits. [`NoParams`] refuses a non-empty
//! body rather than ignoring it: trailing bytes here would mean the program
//! grew an argument, which is exactly the change that must announce itself.
//!
//! # Accounts
//!
//! Deliberately no account struct. The IDL declares 5; mainnet has been
//! observed sending more, the same drift the v2 swap instructions show. A
//! fixed-slot struct would decode the wrong pubkeys the moment the program adds
//! one, and the event names its own participants anyway.

/// Arguments: none.
///
/// A distinct type rather than an alias to a shared zero-argument marker,
/// because the extraction traits are implemented per params type and two
/// instructions with different events cannot share one. That is the same reason
/// each discriminator has its own file: shared types answer one question for
/// instructions that do not agree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CollectCreatorFeeParams;

impl CollectCreatorFeeParams {
    /// Argument encoding: empty. The discriminator is the whole instruction.
    #[must_use]
    pub fn to_data(self) -> Vec<u8> {
        Vec::new()
    }
}

impl crate::parsing::FromInstructionData for CollectCreatorFeeParams {
    fn from_instruction_data(data: &[u8]) -> Result<Self, crate::parsing::InstructionParseError> {
        if data.is_empty() {
            return Ok(Self);
        }
        Err(
            crate::parsing::InstructionParseError::DeserializationFailed(format!(
                "CollectCreatorFeeParams takes no arguments, got {} bytes",
                data.len()
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    /// Trailing bytes mean the program grew an argument — exactly the change
    /// that must announce itself rather than being ignored.
    #[test]
    fn no_arguments_means_no_bytes() {
        assert!(CollectCreatorFeeParams::from_instruction_data(&[]).is_ok());
        assert!(CollectCreatorFeeParams::from_instruction_data(&[0]).is_err());
    }
}
