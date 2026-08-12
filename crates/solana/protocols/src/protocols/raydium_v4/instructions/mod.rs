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
    DEPOSIT_IX, INITIALIZE2_IX, SWAP_BASE_IN_IX, SWAP_BASE_OUT_IX, WITHDRAW_IX,
};
use crate::parsing::{FromAccountKeys, FromInstructionData, InstructionParseError};
use solana_program::pubkey::Pubkey;

/// Raydium V4 instruction variants.
///
/// Uses manual `try_from_slice` because Raydium V4 uses 1-byte instruction
/// indices, not 8-byte Anchor discriminators.
#[derive(Debug, Clone)]
pub enum RaydiumV4Instruction {
    /// Swap with exact input amount.
    SwapBaseIn(SwapBaseInParams),
    /// Swap with exact output amount.
    SwapBaseOut(SwapBaseOutParams),
    /// Create a new AMM pool with initial liquidity.
    Initialize2(Initialize2Params),
    /// Add liquidity to an existing pool.
    Deposit(DepositParams),
    /// Remove liquidity from a pool.
    Withdraw(WithdrawParams),
}

impl RaydiumV4Instruction {
    /// Try to parse instruction from data bytes.
    ///
    /// # Errors
    ///
    /// Returns error if instruction index is unrecognized or data is invalid.
    pub fn try_from_slice(data: &[u8]) -> Result<Self, InstructionParseError> {
        if data.is_empty() {
            return Err(InstructionParseError::DataTooShort);
        }

        let ix_index = data[0];

        // Strip the 1-byte discriminator before passing to from_instruction_data
        // (trait contract: data is after discriminator)
        let params_data = &data[1..];

        match ix_index {
            SWAP_BASE_IN_IX => {
                let params =
                    <SwapBaseInParams as FromInstructionData>::from_instruction_data(params_data)?;
                Ok(RaydiumV4Instruction::SwapBaseIn(params))
            }
            SWAP_BASE_OUT_IX => {
                let params =
                    <SwapBaseOutParams as FromInstructionData>::from_instruction_data(params_data)?;
                Ok(RaydiumV4Instruction::SwapBaseOut(params))
            }
            INITIALIZE2_IX => {
                let params =
                    <Initialize2Params as FromInstructionData>::from_instruction_data(params_data)?;
                Ok(RaydiumV4Instruction::Initialize2(params))
            }
            DEPOSIT_IX => {
                let params =
                    <DepositParams as FromInstructionData>::from_instruction_data(params_data)?;
                Ok(RaydiumV4Instruction::Deposit(params))
            }
            WITHDRAW_IX => {
                let params =
                    <WithdrawParams as FromInstructionData>::from_instruction_data(params_data)?;
                Ok(RaydiumV4Instruction::Withdraw(params))
            }
            _ => Err(InstructionParseError::UnknownDiscriminatorVec(vec![
                ix_index,
            ])),
        }
    }

    /// Get the instruction discriminator (1 byte for Raydium V4).
    #[must_use]
    pub fn discriminator(&self) -> [u8; 1] {
        match self {
            RaydiumV4Instruction::SwapBaseIn(_) => [SWAP_BASE_IN_IX],
            RaydiumV4Instruction::SwapBaseOut(_) => [SWAP_BASE_OUT_IX],
            RaydiumV4Instruction::Initialize2(_) => [INITIALIZE2_IX],
            RaydiumV4Instruction::Deposit(_) => [DEPOSIT_IX],
            RaydiumV4Instruction::Withdraw(_) => [WITHDRAW_IX],
        }
    }

    /// Serialize instruction to bytes.
    #[must_use]
    pub fn data(&self) -> Vec<u8> {
        match self {
            RaydiumV4Instruction::SwapBaseIn(params) => params.to_data(),
            RaydiumV4Instruction::SwapBaseOut(params) => params.to_data(),
            RaydiumV4Instruction::Initialize2(params) => params.to_data(),
            RaydiumV4Instruction::Deposit(params) => params.to_data(),
            RaydiumV4Instruction::Withdraw(params) => params.to_data(),
        }
    }

    /// Parse accounts for this instruction.
    pub fn from_accounts(
        &self,
        account_keys: &[Pubkey],
    ) -> Result<RaydiumV4InstructionAccounts, InstructionParseError> {
        match self {
            RaydiumV4Instruction::SwapBaseIn(_) => {
                let accounts = SwapBaseInAccounts::from_account_keys(account_keys)?;
                Ok(RaydiumV4InstructionAccounts::SwapBaseIn(accounts))
            }
            RaydiumV4Instruction::SwapBaseOut(_) => {
                let accounts = SwapBaseOutAccounts::from_account_keys(account_keys)?;
                Ok(RaydiumV4InstructionAccounts::SwapBaseOut(accounts))
            }
            RaydiumV4Instruction::Initialize2(_) => {
                let accounts = Initialize2Accounts::from_account_keys(account_keys)?;
                Ok(RaydiumV4InstructionAccounts::Initialize2(accounts))
            }
            RaydiumV4Instruction::Deposit(_) => {
                let accounts = DepositAccounts::from_account_keys(account_keys)?;
                Ok(RaydiumV4InstructionAccounts::Deposit(accounts))
            }
            RaydiumV4Instruction::Withdraw(_) => {
                let accounts = WithdrawAccounts::from_account_keys(account_keys)?;
                Ok(RaydiumV4InstructionAccounts::Withdraw(accounts))
            }
        }
    }

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

/// Raydium V4 instruction accounts enum.
#[derive(Debug, Clone)]
pub enum RaydiumV4InstructionAccounts {
    /// SwapBaseIn accounts.
    SwapBaseIn(SwapBaseInAccounts),
    /// SwapBaseOut accounts.
    SwapBaseOut(SwapBaseOutAccounts),
    /// Initialize2 accounts.
    Initialize2(Initialize2Accounts),
    /// Deposit accounts.
    Deposit(DepositAccounts),
    /// Withdraw accounts.
    Withdraw(WithdrawAccounts),
}

#[cfg(test)]
mod tests {
    use super::*;

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
