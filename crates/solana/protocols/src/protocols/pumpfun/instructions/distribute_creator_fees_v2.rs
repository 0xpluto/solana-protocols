//! Pump.fun `distribute_creator_fees_v2`.
//!
//! The distribution again, able to create the recipient's associated token
//! account on the way — which is the one argument it takes, and the reason this
//! is a separate file from `distribute_creator_fees` rather than a shared
//! zero-argument type. The v1 form takes nothing; this takes a bool. A single
//! params type would have to be wrong for one of them.

use serde::{Deserialize, Serialize};

use crate::parsing::{FromInstructionData, InstructionParseError};

/// Arguments for `distribute_creator_fees_v2`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributeCreatorFeesV2Params {
    /// Whether the program should create the creator's associated token account
    /// as part of the distribution.
    pub initialize_ata: bool,
}

impl DistributeCreatorFeesV2Params {
    /// Argument encoding: the single bool.
    #[must_use]
    pub fn to_data(self) -> Vec<u8> {
        vec![u8::from(self.initialize_ata)]
    }
}

impl FromInstructionData for DistributeCreatorFeesV2Params {
    fn from_instruction_data(data: &[u8]) -> Result<Self, InstructionParseError> {
        // Not `!= 0`: a byte outside {0, 1} is a layout disagreement, and
        // coercing it to `true` would hide the argument changing shape.
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

    #[test]
    fn the_bool_argument_refuses_anything_but_zero_or_one() {
        assert!(
            DistributeCreatorFeesV2Params::from_instruction_data(&[1])
                .expect("true")
                .initialize_ata
        );
        assert!(
            !DistributeCreatorFeesV2Params::from_instruction_data(&[0])
                .expect("false")
                .initialize_ata
        );
        for bad in [&[][..], &[2][..], &[1, 0][..]] {
            assert!(
                DistributeCreatorFeesV2Params::from_instruction_data(bad).is_err(),
                "{bad:?}"
            );
        }
    }
}
