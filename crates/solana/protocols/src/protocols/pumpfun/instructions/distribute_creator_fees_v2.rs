//! Pump.fun `distribute_creator_fees_v2`.
//!
//! The distribution again, able to create the recipient's associated token
//! account on the way — which is the one argument it takes, and the reason this
//! is a separate file from `distribute_creator_fees` rather than a shared
//! zero-argument type. The v1 form takes nothing; this takes a bool. A single
//! params type would have to be wrong for one of them.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData, OnchainInstruction};

/// Arguments for `distribute_creator_fees_v2`.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    InstructionData,
)]
#[instruction_data(discriminator = super::super::constants::DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR, fixtures(
    "pumpfun/ix_distribute_creator_fees_v2_n13.json"
), idl(program = "pump", instruction = "distribute_creator_fees_v2"))]
pub struct DistributeCreatorFeesV2Params {
    /// Whether the program should create the creator's associated token account
    /// as part of the distribution.
    pub initialize_ata: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::FromInstructionData;

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

/// Accounts for `distribute_creator_fees_v2` — 12 slots from the IDL.
///
/// This layout is a **hypothesis**. The file previously declared no accounts
/// struct at all, on the reasoning that mainnet sends more slots than the IDL
/// declares and a fixed-slot struct would decode the wrong pubkeys. The capture
/// says otherwise, except for one appended account kept in `unidentified`.
///
/// Modelled anyway, because not modelling it records nothing: the creator-fee
/// accounts are functionality thrown away, and a struct that is wrong fails its
/// golden fixture and says so, while an absent struct fails nothing and teaches
/// nobody. If the program sends a slot we do not expect, `UnmodelledAccounts`
/// refuses it.
#[derive(Debug, Clone, AccountMetas, OnchainInstruction)]
#[idl(program = "pump", instruction = "distribute_creator_fees_v2")]
#[onchain_ix(fixtures("pumpfun/ix_distribute_creator_fees_v2_n13.json"))]
pub struct DistributeCreatorFeesV2Accounts {
    /// IDL slot 0.
    #[account(writable, signer)]
    pub payer: Pubkey,
    /// IDL slot 1.
    #[account]
    pub mint: Pubkey,
    /// IDL slot 2.
    #[account]
    pub bonding_curve: Pubkey,
    /// IDL slot 3.
    #[account]
    pub sharing_config: Pubkey,
    /// IDL slot 4.
    #[account(writable)]
    pub creator_vault: Pubkey,
    /// IDL slot 5.
    #[account]
    pub system_program: Pubkey,
    /// IDL slot 6.
    #[account]
    pub event_authority: Pubkey,
    /// IDL slot 7.
    #[account]
    pub program: Pubkey,
    /// IDL slot 8.
    #[account(writable)]
    pub creator_vault_quote_token_account: Pubkey,
    /// IDL slot 9.
    #[account]
    pub quote_mint: Pubkey,
    /// IDL slot 10.
    #[account]
    pub quote_token_program: Pubkey,
    /// IDL slot 11.
    #[account]
    pub associated_token_program: Pubkey,
    /// Appended past the IDL's list.
    #[account(
        remaining,
        reason = "one appended account observed on every capture; in the captured instruction it repeats the payer at slot 0, which is real but unexplained, so it is recorded rather than named"
    )]
    pub unidentified: Vec<Pubkey>,
}
