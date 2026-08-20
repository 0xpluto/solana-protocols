//! Raydium V4 instruction definitions.
//!
//! Raydium V4 uses 1-byte instruction indices instead of 8-byte Anchor discriminators.

mod deposit;
mod initialize2;
mod swap;
mod withdraw;

pub use deposit::{DepositAccounts, DepositParams};
pub use initialize2::{Initialize2Accounts, Initialize2Params};
pub use swap::{
    SwapBaseInAccounts, SwapBaseInBuilder, SwapBaseInParams, SwapBaseOutAccounts, SwapBaseOutParams,
};
pub use withdraw::{WithdrawAccounts, WithdrawParams};

use super::constants::{
    DEPOSIT_IX, INITIALIZE2_IX, PROGRAM_ID, SWAP_BASE_IN_IX, SWAP_BASE_OUT_IX, WITHDRAW_IX,
};
use solana_protocols_macros::ProtocolInstruction;

/// Raydium V4 instruction variants.
///
/// Raydium V4 uses a 1-byte instruction index rather than an 8-byte Anchor
/// discriminator. That is a declared parameter — `discriminator_size = 1` — not
/// a reason to hand-roll dispatch: this module did so for years behind a comment
/// saying the macro could not express it, which stopped being true once
/// `spl_token` needed the same thing.
#[derive(Debug, Clone, ProtocolInstruction)]
#[protocol(program_id = PROGRAM_ID, discriminator_size = 1)]
pub enum RaydiumV4Instruction {
    /// Swap with exact input amount.
    #[instruction(discriminator = [SWAP_BASE_IN_IX], accounts = SwapBaseInAccounts)]
    SwapBaseIn(SwapBaseInParams),
    /// Swap with exact output amount.
    #[instruction(discriminator = [SWAP_BASE_OUT_IX], accounts = SwapBaseOutAccounts)]
    SwapBaseOut(SwapBaseOutParams),
    /// Create a new AMM pool with initial liquidity.
    #[instruction(discriminator = [INITIALIZE2_IX], accounts = Initialize2Accounts)]
    Initialize2(Initialize2Params),
    /// Add liquidity to an existing pool.
    #[instruction(discriminator = [DEPOSIT_IX], accounts = DepositAccounts)]
    Deposit(DepositParams),
    /// Remove liquidity from a pool.
    #[instruction(discriminator = [WITHDRAW_IX], accounts = WithdrawAccounts)]
    Withdraw(WithdrawParams),
}

impl RaydiumV4Instruction {
    /// Check if this is a swap instruction.
    #[must_use]
    pub fn is_swap(&self) -> bool {
        matches!(
            self,
            RaydiumV4Instruction::SwapBaseIn(_) | RaydiumV4Instruction::SwapBaseOut(_)
        )
    }

    /// Check if this is an initialize2 instruction.
    #[must_use]
    pub fn is_initialize2(&self) -> bool {
        matches!(self, RaydiumV4Instruction::Initialize2(_))
    }

    /// Check if this is a deposit instruction.
    #[must_use]
    pub fn is_deposit(&self) -> bool {
        matches!(self, RaydiumV4Instruction::Deposit(_))
    }

    /// Check if this is a withdraw instruction.
    #[must_use]
    pub fn is_withdraw(&self) -> bool {
        matches!(self, RaydiumV4Instruction::Withdraw(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    #[test]
    fn swap_base_in_params_to_data() {
        let params = SwapBaseInParams::new(1_000_000_000, 900_000_000);
        let data = params.to_data();

        assert_eq!(data.len(), 17); // 1 + 8 + 8
        assert_eq!(data[0], SWAP_BASE_IN_IX);

        let amount_in = u64::from_le_bytes(data[1..9].try_into().unwrap());
        let min_out = u64::from_le_bytes(data[9..17].try_into().unwrap());
        assert_eq!(amount_in, 1_000_000_000);
        assert_eq!(min_out, 900_000_000);
    }

    #[test]
    fn swap_base_in_params_from_data() {
        let original = SwapBaseInParams::new(1_000_000_000, 900_000_000);
        let data = original.to_data();

        let parsed = SwapBaseInParams::from_instruction_data(&data[1..]).unwrap();
        assert_eq!(parsed.amount_in, 1_000_000_000);
        assert_eq!(parsed.minimum_amount_out, 900_000_000);
    }

    #[test]
    fn swap_base_out_params_to_data() {
        let params = SwapBaseOutParams::new(1_100_000_000, 1_000_000_000);
        let data = params.to_data();

        assert_eq!(data.len(), 17); // 1 + 8 + 8
        assert_eq!(data[0], SWAP_BASE_OUT_IX);
    }

    #[test]
    fn instruction_try_from_slice() {
        // SwapBaseIn
        let data = SwapBaseInParams::new(1_000_000_000, 900_000_000).to_data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        assert!(matches!(ix, RaydiumV4Instruction::SwapBaseIn(_)));

        // SwapBaseOut
        let data = SwapBaseOutParams::new(1_100_000_000, 1_000_000_000).to_data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        assert!(matches!(ix, RaydiumV4Instruction::SwapBaseOut(_)));

        // Initialize2
        let data = Initialize2Params::new(255, 1_700_000_000, 500_000_000, 1_000_000).to_data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        assert!(matches!(ix, RaydiumV4Instruction::Initialize2(_)));

        // Deposit
        let data = DepositParams::new(1_000_000, 500_000_000, 0).to_data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        assert!(matches!(ix, RaydiumV4Instruction::Deposit(_)));

        // Withdraw
        let data = WithdrawParams::new(100_000).to_data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        assert!(matches!(ix, RaydiumV4Instruction::Withdraw(_)));

        // Unknown instruction
        let data = vec![99u8; 17];
        let result = RaydiumV4Instruction::try_from_slice(&data);
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_initialize2() {
        let params = Initialize2Params::new(42, 1_700_000_000, 500_000_000, 1_000_000);
        let data = RaydiumV4Instruction::Initialize2(params).data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        match ix {
            RaydiumV4Instruction::Initialize2(p) => {
                assert_eq!(p.nonce, 42);
                assert_eq!(p.open_time, 1_700_000_000);
                assert_eq!(p.init_pc_amount, 500_000_000);
                assert_eq!(p.init_coin_amount, 1_000_000);
            }
            RaydiumV4Instruction::SwapBaseIn(_)
            | RaydiumV4Instruction::SwapBaseOut(_)
            | RaydiumV4Instruction::Deposit(_)
            | RaydiumV4Instruction::Withdraw(_) => panic!("expected Initialize2"),
        }
    }

    #[test]
    fn roundtrip_deposit() {
        let params = DepositParams::new(1_000_000, 500_000_000, 1);
        let data = RaydiumV4Instruction::Deposit(params).data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        match ix {
            RaydiumV4Instruction::Deposit(p) => {
                assert_eq!(p.max_coin_amount, 1_000_000);
                assert_eq!(p.max_pc_amount, 500_000_000);
                assert_eq!(p.base_side, 1);
            }
            RaydiumV4Instruction::SwapBaseIn(_)
            | RaydiumV4Instruction::SwapBaseOut(_)
            | RaydiumV4Instruction::Initialize2(_)
            | RaydiumV4Instruction::Withdraw(_) => panic!("expected Deposit"),
        }
    }

    #[test]
    fn roundtrip_withdraw() {
        let params = WithdrawParams::new(100_000);
        let data = RaydiumV4Instruction::Withdraw(params).data();
        let ix = RaydiumV4Instruction::try_from_slice(&data).unwrap();
        match ix {
            RaydiumV4Instruction::Withdraw(p) => {
                assert_eq!(p.amount, 100_000);
            }
            RaydiumV4Instruction::SwapBaseIn(_)
            | RaydiumV4Instruction::SwapBaseOut(_)
            | RaydiumV4Instruction::Initialize2(_)
            | RaydiumV4Instruction::Deposit(_) => panic!("expected Withdraw"),
        }
    }
}
