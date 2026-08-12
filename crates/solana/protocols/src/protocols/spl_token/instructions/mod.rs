//! SPL Token instruction modules.
//!
//! Each instruction gets its own file with accounts struct + params struct.
//! The `ProtocolInstruction` derive macro generates dispatch with 1-byte
//! discriminators (SPL Token uses single-byte instruction indices).

pub mod transfer;
pub mod transfer_checked;

pub use transfer::{TransferAccounts, TransferParams};
pub use transfer_checked::{TransferCheckedAccounts, TransferCheckedParams};

use solana_protocols_macros::ProtocolInstruction;

use super::constants::{PROGRAM_ID, TRANSFER_CHECKED_DISCRIMINATOR, TRANSFER_DISCRIMINATOR};

/// SPL Token instruction enum (transfer variants only).
///
/// `#[derive(ProtocolInstruction)]` with `discriminator_size = 1` generates:
/// - `try_from_slice(data)` — 1-byte discriminator dispatch
/// - `discriminator()` — get 1-byte discriminator
/// - `data()` — serialize back to bytes
/// - `from_accounts(keys)` — parse accounts via `FromAccountKeys`
/// - `SplTokenInstructionAccounts` enum
/// - `SplTokenInstructionEvent` struct
#[derive(Debug, Clone, ProtocolInstruction)]
#[protocol(program_id = PROGRAM_ID, discriminator_size = 1)]
pub enum SplTokenInstruction {
    /// Transfer tokens between accounts.
    #[instruction(discriminator = TRANSFER_DISCRIMINATOR, accounts = TransferAccounts)]
    Transfer(TransferParams),
    /// Transfer tokens with mint verification.
    #[instruction(discriminator = TRANSFER_CHECKED_DISCRIMINATOR, accounts = TransferCheckedAccounts)]
    TransferChecked(TransferCheckedParams),
}

impl SplTokenInstruction {
    /// Get the transfer amount regardless of variant.
    #[must_use]
    pub fn amount(&self) -> u64 {
        match self {
            SplTokenInstruction::Transfer(p) => p.amount,
            SplTokenInstruction::TransferChecked(p) => p.amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transfer_data(amount: u64) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&TRANSFER_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        data
    }

    fn make_transfer_checked_data(amount: u64, decimals: u8) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&TRANSFER_CHECKED_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(decimals);
        data
    }

    #[test]
    fn parse_transfer_instruction() {
        let data = make_transfer_data(1_000_000);
        let ix = SplTokenInstruction::try_from_slice(&data).unwrap();

        match &ix {
            SplTokenInstruction::Transfer(params) => {
                assert_eq!(params.amount, 1_000_000);
            }
            SplTokenInstruction::TransferChecked(_) => panic!("expected Transfer"),
        }
        assert_eq!(ix.amount(), 1_000_000);
    }

    #[test]
    fn parse_transfer_checked_instruction() {
        let data = make_transfer_checked_data(500_000, 6);
        let ix = SplTokenInstruction::try_from_slice(&data).unwrap();

        match &ix {
            SplTokenInstruction::TransferChecked(params) => {
                assert_eq!(params.amount, 500_000);
                assert_eq!(params.decimals, 6);
            }
            SplTokenInstruction::Transfer(_) => panic!("expected TransferChecked"),
        }
        assert_eq!(ix.amount(), 500_000);
    }

    #[test]
    fn parse_unknown_discriminator() {
        let data = vec![0xFF; 12];
        assert!(SplTokenInstruction::try_from_slice(&data).is_err());
    }

    #[test]
    fn parse_too_short() {
        assert!(SplTokenInstruction::try_from_slice(&[]).is_err());
    }

    #[test]
    fn roundtrip_via_data() {
        let original = SplTokenInstruction::Transfer(TransferParams::new(42));
        let data = original.data();
        let parsed = SplTokenInstruction::try_from_slice(&data).unwrap();
        assert_eq!(parsed.amount(), 42);
    }

    #[test]
    fn discriminator_values() {
        let transfer = SplTokenInstruction::Transfer(TransferParams::new(0));
        assert_eq!(transfer.discriminator(), [3]);

        let checked = SplTokenInstruction::TransferChecked(TransferCheckedParams::new(0, 6));
        assert_eq!(checked.discriminator(), [12]);
    }
}
