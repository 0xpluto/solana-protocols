//! Pump.fun `migrate` — a bonding curve graduating to the PumpSwap AMM.
//!
//! The pumpfun side of a fact that spans two programs: this instruction CPIs
//! into pump_amm's `create_pool`, so the graduation was already recorded from
//! the AMM side. What was missing is pumpfun's own view of it — and with it the
//! `CompletePumpAmmMigrationEvent`, which names the source bonding curve and the
//! `pool_migration_fee` that the pumpswap event does not carry.
//!
//! Takes no arguments: everything is in the accounts and the event.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::{AccountMetas, InstructionData};


/// Arguments: none.
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
#[instruction_data(discriminator = super::super::constants::MIGRATE_DISCRIMINATOR, unverified = "the v1 migration; not seen in any firehose capture", idl(program = "pump", instruction = "migrate"))]
pub struct MigrateParams;

/// Accounts for `migrate` — 25 slots from the IDL.
#[derive(Debug, Clone, AccountMetas)]
#[idl(program = "pump", instruction = "migrate")]
#[accounts(unverified = "the v1 migration; superseded by migrate_v2 and not seen in any firehose capture, so there is no real instruction to pin it against")]
pub struct MigrateAccounts {
    /// IDL slot 0.
    #[account]
    pub global: Pubkey,
    /// IDL slot 1.
    #[account(writable)]
    pub withdraw_authority: Pubkey,
    /// IDL slot 2.
    #[account]
    pub mint: Pubkey,
    /// IDL slot 3.
    #[account(writable)]
    pub bonding_curve: Pubkey,
    /// IDL slot 4.
    #[account(writable)]
    pub associated_bonding_curve: Pubkey,
    /// IDL slot 5.
    #[account(signer)]
    pub user: Pubkey,
    /// IDL slot 6.
    #[account]
    pub system_program: Pubkey,
    /// IDL slot 7.
    #[account]
    pub token_program: Pubkey,
    /// IDL slot 8.
    #[account]
    pub pump_amm: Pubkey,
    /// IDL slot 9.
    #[account(writable)]
    pub pool: Pubkey,
    /// IDL slot 10.
    #[account(writable)]
    pub pool_authority: Pubkey,
    /// IDL slot 11.
    #[account(writable)]
    pub pool_authority_mint_account: Pubkey,
    /// IDL slot 12.
    #[account(writable)]
    pub pool_authority_wsol_account: Pubkey,
    /// IDL slot 13.
    #[account]
    pub amm_global_config: Pubkey,
    /// IDL slot 14.
    #[account]
    pub wsol_mint: Pubkey,
    /// IDL slot 15.
    #[account(writable)]
    pub lp_mint: Pubkey,
    /// IDL slot 16.
    #[account(writable)]
    pub user_pool_token_account: Pubkey,
    /// IDL slot 17.
    #[account(writable)]
    pub pool_base_token_account: Pubkey,
    /// IDL slot 18.
    #[account(writable)]
    pub pool_quote_token_account: Pubkey,
    /// IDL slot 19.
    #[account]
    pub token_2022_program: Pubkey,
    /// IDL slot 20.
    #[account]
    pub associated_token_program: Pubkey,
    /// IDL slot 21.
    #[account]
    pub pump_amm_event_authority: Pubkey,
    /// IDL slot 22.
    #[account]
    pub event_authority: Pubkey,
    /// IDL slot 23.
    #[account]
    pub program: Pubkey,
    /// IDL slot 24.
    #[account]
    pub rent: Pubkey,
    /// Appended past the IDL's list.
    #[account(
        remaining,
        reason = "two accounts appended past the declared list on the captured \
                  migration; not identified, so recorded rather than named — the \
                  amounts come from CompletePumpAmmMigrationEvent either way"
    )]
    pub unidentified: Vec<Pubkey>,
}
