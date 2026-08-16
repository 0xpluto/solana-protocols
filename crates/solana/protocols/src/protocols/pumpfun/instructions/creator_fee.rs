//! Pump.fun creator-fee instructions.
//!
//! Four ways accrued creator fees leave their vault. All of them are
//! **identified, not dissected**: three take no arguments at all and the fourth
//! takes one bool, so the instruction data carries no economics. Everything
//! worth recording — who was paid, how much, in what — is in the event these
//! emit, which is why there are no account structs here.
//!
//! That is deliberate rather than lazy. Mainnet 2026-08-15 shows these carrying
//! *more* accounts than the IDL declares (`distribute_creator_fees` at 8 against
//! 7, `distribute_creator_fees_v2` at 13 against 12), the same drift the v2 swap
//! instructions show. A fixed-slot account struct would decode the wrong
//! pubkeys the moment the program adds a slot; the event names its own
//! participants and cannot.

use serde::{Deserialize, Serialize};

use crate::parsing::{FromInstructionData, InstructionParseError};

/// Zero-argument creator-fee instructions share the shared marker type.
pub use crate::parsing::NoParams as CreatorFeeParams;

/// `distribute_creator_fees_v2` — one declared argument.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributeCreatorFeesV2Params {
    /// `initialize_ata` — whether the program should create the creator's
    /// associated token account as part of the distribution.
    pub initialize_ata: bool,
}

impl FromInstructionData for DistributeCreatorFeesV2Params {
    fn from_instruction_data(data: &[u8]) -> Result<Self, InstructionParseError> {
        match data {
            [0] => Ok(Self {
                initialize_ata: false,
            }),
            [1] => Ok(Self {
                initialize_ata: true,
            }),
            other => Err(InstructionParseError::DeserializationFailed(format!(
                "distribute_creator_fees_v2 initialize_ata: expected one bool byte, got {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not `!= 0`: a byte outside `{0, 1}` is a layout disagreement, and
    /// coercing it to `true` would hide the argument changing shape.
    #[test]
    fn the_bool_argument_refuses_anything_but_zero_or_one() {
        assert_eq!(
            DistributeCreatorFeesV2Params::from_instruction_data(&[1]).expect("true byte"),
            DistributeCreatorFeesV2Params {
                initialize_ata: true
            }
        );
        assert_eq!(
            DistributeCreatorFeesV2Params::from_instruction_data(&[0]).expect("false byte"),
            DistributeCreatorFeesV2Params {
                initialize_ata: false
            }
        );
        assert!(DistributeCreatorFeesV2Params::from_instruction_data(&[2]).is_err());
        assert!(DistributeCreatorFeesV2Params::from_instruction_data(&[]).is_err());
        assert!(DistributeCreatorFeesV2Params::from_instruction_data(&[1, 0]).is_err());
    }
}
