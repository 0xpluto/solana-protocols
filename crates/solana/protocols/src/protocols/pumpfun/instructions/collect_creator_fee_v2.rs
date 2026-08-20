//! Pump.fun `collect_creator_fee_v2`.
//!
//! The same withdrawal, settling into a token account rather than a bare lamport transfer.
//!
//! Zero arguments, so the instruction data carries no economics — everything
//! worth recording is in the event it emits. [`NoParams`] refuses a non-empty
//! body rather than ignoring it: trailing bytes here would mean the program
//! grew an argument, which is exactly the change that must announce itself.
//!
//! # Accounts
//!
//! This block used to read "deliberately no account struct: the IDL declares
//! 10; mainnet has been observed sending more". The capture says otherwise —
//! real instructions carry exactly 10. The claim was never
//! measured, and it cost the account layout entirely: no struct meant no
//! fixture, no IDL name check, and no way for any of it to be wrong out loud.
//!
//! The layout is now modelled and pinned to a real landed instruction. If the
//! program does add a slot, `UnmodelledAccounts` refuses it and says so.

/// Arguments: none.
///
/// A distinct type rather than an alias to a shared zero-argument marker,
/// because the extraction traits are implemented per params type and two
/// instructions with different events cannot share one. That is the same reason
/// each discriminator has its own file: shared types answer one question for
/// instructions that do not agree.
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData, OnchainInstruction};
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = super::super::constants::COLLECT_CREATOR_FEE_V2_DISCRIMINATOR, fixtures(
    "pumpfun/ix_collect_creator_fee_v2_n10.json"
), idl(program = "pump", instruction = "collect_creator_fee_v2"))]
pub struct CollectCreatorFeeV2Params;



#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

    /// Trailing bytes mean the program grew an argument — exactly the change
    /// that must announce itself rather than being ignored.
    #[test]
    fn no_arguments_means_no_bytes() {
        assert!(CollectCreatorFeeV2Params::from_instruction_data(&[]).is_ok());
        assert!(CollectCreatorFeeV2Params::from_instruction_data(&[0]).is_err());
    }
}

/// Accounts for `collect_creator_fee_v2` — 10 slots from the IDL.
///
/// This layout is a **hypothesis**. The file previously declared no accounts
/// struct at all, on the reasoning that mainnet sends more slots than the IDL
/// declares and a fixed-slot struct would decode the wrong pubkeys. The capture
/// says otherwise: this instruction arrives at exactly the declared count.
///
/// Modelled anyway, because not modelling it records nothing: the creator-fee
/// accounts are functionality thrown away, and a struct that is wrong fails its
/// golden fixture and says so, while an absent struct fails nothing and teaches
/// nobody. If the program sends a slot we do not expect, `UnmodelledAccounts`
/// refuses it — deliberately, since there is no `remaining` field here to absorb a surprise.
#[derive(Debug, Clone, AccountMetas, OnchainInstruction)]
#[idl(program = "pump", instruction = "collect_creator_fee_v2")]
#[onchain_ix(fixtures("pumpfun/ix_collect_creator_fee_v2_n10.json"))]
pub struct CollectCreatorFeeV2Accounts {
    /// IDL slot 0.
    #[account(writable)]
    pub creator: Pubkey,
    /// IDL slot 1.
    #[account(writable)]
    pub creator_token_account: Pubkey,
    /// IDL slot 2.
    #[account(writable)]
    pub creator_vault: Pubkey,
    /// IDL slot 3.
    #[account(writable)]
    pub creator_vault_token_account: Pubkey,
    /// IDL slot 4.
    #[account]
    pub quote_mint: Pubkey,
    /// IDL slot 5.
    #[account]
    pub quote_token_program: Pubkey,
    /// IDL slot 6.
    #[account]
    pub associated_token_program: Pubkey,
    /// IDL slot 7.
    #[account]
    pub system_program: Pubkey,
    /// IDL slot 8.
    #[account]
    pub event_authority: Pubkey,
    /// IDL slot 9.
    #[account]
    pub program: Pubkey,
}
