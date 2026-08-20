//! Pumpfun `Global` PDA account layout.
//!
//! The Global PDA at [`GLOBAL_PDA`](super::constants::GLOBAL_PDA) holds
//! protocol-wide configuration. Two fields matter for quoting /
//! instruction building:
//!
//! * `fee_recipients\[0\]` — the active fee recipient for **regular** curves.
//! * `reserved_fee_recipients\[0\]` — the active fee recipient for curves
//!   with `is_mayhem_mode = true`.
//!
//! Pumpfun validates the fee recipient passed to a buy/sell instruction
//! against the appropriate array based on the bonding curve's mayhem
//! flag. Using the wrong recipient → transaction reverts.

use borsh::BorshDeserialize;
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::OnchainState;

/// Full layout of the Pumpfun Global state account.
///
/// Deserialized from the 8-byte Anchor discriminator-prefixed account
/// data. Layout (Borsh, after the discriminator):
///
/// | Field                          | Type         | Bytes |
/// |--------------------------------|--------------|-------|
/// | `initialized`                  | bool         | 1     |
/// | `authority`                    | Pubkey       | 32    |
/// | `fee_recipient`                | Pubkey       | 32    |
/// | `initial_virtual_token_reserves` | u64        | 8     |
/// | `initial_virtual_sol_reserves` | u64          | 8     |
/// | `initial_real_token_reserves`  | u64          | 8     |
/// | `token_total_supply`           | u64          | 8     |
/// | `fee_basis_points`             | u64          | 8     |
/// | `withdraw_authority`           | Pubkey       | 32    |
/// | `enable_migrate`               | bool         | 1     |
/// | `pool_migration_fee`           | u64          | 8     |
/// | `creator_fee_basis_points`     | u64          | 8     |
/// | `fee_recipients`               | [Pubkey; 7]  | 224   |
/// | `set_creator_authority`        | Pubkey       | 32    |
/// | `admin_set_creator_authority`  | Pubkey       | 32    |
/// | `create_v2_enabled`            | bool         | 1     |
/// | `whitelist_pda`                | Pubkey       | 32    |
/// | `reserved_fee_recipient`       | Pubkey       | 32    |
/// | `mayhem_mode_enabled`          | bool         | 1     |
/// | `reserved_fee_recipients`      | [Pubkey; 7]  | 224   |
/// | `is_cashback_enabled`          | bool         | 1     |
#[derive(BorshDeserialize, Debug, Clone, serde::Serialize, OnchainState)]
#[idl(program = "pump", account = "Global")]
#[state(discriminator = super::constants::GLOBAL_DISCRIMINATOR)]
#[state(fixtures("pumpfun/global.json"))]
pub struct PumpfunGlobal {
    pub initialized: bool,
    pub authority: Pubkey,
    pub fee_recipient: Pubkey,
    pub initial_virtual_token_reserves: u64,
    pub initial_virtual_sol_reserves: u64,
    pub initial_real_token_reserves: u64,
    pub token_total_supply: u64,
    pub fee_basis_points: u64,
    pub withdraw_authority: Pubkey,
    pub enable_migrate: bool,
    pub pool_migration_fee: u64,
    pub creator_fee_basis_points: u64,
    pub fee_recipients: [Pubkey; 7],
    pub set_creator_authority: Pubkey,
    pub admin_set_creator_authority: Pubkey,
    pub create_v2_enabled: bool,
    pub whitelist_pda: Pubkey,
    pub reserved_fee_recipient: Pubkey,
    pub mayhem_mode_enabled: bool,
    pub reserved_fee_recipients: [Pubkey; 7],
    pub is_cashback_enabled: bool,
}

impl PumpfunGlobal {
    /// Parse from raw account data (8-byte discriminator prefix included).
    ///
    /// # Errors
    ///
    /// The discriminator does not match, or the body does not decode. This
    /// returned `Option` and logged the reason at `warn!`, which the default
    /// `solana_protocols=error` filter made unreachable — and it checked no
    /// discriminator at all, so any account of the right length decoded.
    pub fn from_account_data(
        data: &[u8],
    ) -> ::core::result::Result<Self, crate::parsing::state::AccountParseError> {
        <Self as crate::parsing::state::OnchainState>::from_account_data(data)
    }
}

/// Active fee recipients for regular + mayhem mode curves.
///
/// Pumpfun instruction building requires routing the fee_recipient
/// argument to the right Pubkey based on the curve's `is_mayhem_mode`
/// flag. Cached as a singleton in `LocalCache` — see
/// `solana-account-cache`'s `CacheSingleton<PumpfunFeeRecipients>` impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PumpfunFeeRecipients {
    /// `Global::fee_recipients\[0\]` — for curves with `is_mayhem_mode = false`.
    pub regular: Pubkey,
    /// `Global::reserved_fee_recipients\[0\]` — for curves with `is_mayhem_mode = true`.
    pub mayhem: Pubkey,
}

impl PumpfunFeeRecipients {
    /// Extract the two active recipients from a parsed `PumpfunGlobal`.
    pub fn from_global(g: &PumpfunGlobal) -> Self {
        Self {
            regular: g.fee_recipients[0],
            mayhem: g.reserved_fee_recipients[0],
        }
    }

    /// Parse raw account data and extract the two recipients in one shot.
    ///
    /// # Errors
    ///
    /// Whatever [`PumpfunGlobal::from_account_data`] refuses.
    pub fn from_account_data(
        data: &[u8],
    ) -> ::core::result::Result<Self, crate::parsing::state::AccountParseError> {
        PumpfunGlobal::from_account_data(data).map(|g| Self::from_global(&g))
    }

    /// Return the correct fee recipient for a given mayhem flag.
    pub fn for_curve(&self, is_mayhem_mode: bool) -> Pubkey {
        if is_mayhem_mode {
            self.mayhem
        } else {
            self.regular
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    /// The real pumpfun Global PDA decodes and yields two non-default fee
    /// recipients — proves the borsh layout (skip-8 discriminator + field order)
    /// still matches the chain.
    #[test]
    fn decodes_onchain_global() {
        let fx = crate::test_fixtures::AccountFixture::load("pumpfun/global.json");
        let r =
            PumpfunFeeRecipients::from_account_data(fx.data()).expect("decode real pumpfun global");
        assert_ne!(
            r.regular,
            Pubkey::default(),
            "regular fee recipient must be set"
        );
        assert_ne!(
            r.mayhem,
            Pubkey::default(),
            "mayhem fee recipient must be set"
        );
    }

    /// Build a synthetic Global buffer by serializing a filled-out struct.
    /// Since the struct is BorshSerialize via derive, we can round-trip
    /// without depending on a live RPC snapshot.
    #[derive(BorshSerialize)]
    struct GlobalLayoutForEncode {
        initialized: bool,
        authority: Pubkey,
        fee_recipient: Pubkey,
        initial_virtual_token_reserves: u64,
        initial_virtual_sol_reserves: u64,
        initial_real_token_reserves: u64,
        token_total_supply: u64,
        fee_basis_points: u64,
        withdraw_authority: Pubkey,
        enable_migrate: bool,
        pool_migration_fee: u64,
        creator_fee_basis_points: u64,
        fee_recipients: [Pubkey; 7],
        set_creator_authority: Pubkey,
        admin_set_creator_authority: Pubkey,
        create_v2_enabled: bool,
        whitelist_pda: Pubkey,
        reserved_fee_recipient: Pubkey,
        mayhem_mode_enabled: bool,
        reserved_fee_recipients: [Pubkey; 7],
        is_cashback_enabled: bool,
    }

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    fn encoded_global() -> Vec<u8> {
        let mut fee_recipients = [Pubkey::default(); 7];
        fee_recipients[0] = pk(0xAA);
        let mut reserved_fee_recipients = [Pubkey::default(); 7];
        reserved_fee_recipients[0] = pk(0xBB);
        let g = GlobalLayoutForEncode {
            initialized: true,
            authority: pk(0x01),
            fee_recipient: pk(0x02),
            initial_virtual_token_reserves: 1,
            initial_virtual_sol_reserves: 2,
            initial_real_token_reserves: 3,
            token_total_supply: 4,
            fee_basis_points: 95,
            withdraw_authority: pk(0x03),
            enable_migrate: true,
            pool_migration_fee: 0,
            creator_fee_basis_points: 30,
            fee_recipients,
            set_creator_authority: pk(0x04),
            admin_set_creator_authority: pk(0x05),
            create_v2_enabled: true,
            whitelist_pda: pk(0x06),
            reserved_fee_recipient: pk(0x07),
            mayhem_mode_enabled: true,
            reserved_fee_recipients,
            is_cashback_enabled: true,
        };
        // The real discriminator: `from_account_data` checks it now, so a
        // synthetic account built with a zero prefix is correctly refused.
        let mut out = super::super::constants::GLOBAL_DISCRIMINATOR.to_vec();
        out.extend(borsh::to_vec(&g).unwrap());
        out
    }

    #[test]
    fn from_account_data_parses_a_synthetic_global() {
        let data = encoded_global();
        let g = PumpfunGlobal::from_account_data(&data).expect("parse");
        assert_eq!(g.fee_recipients[0], pk(0xAA));
        assert_eq!(g.reserved_fee_recipients[0], pk(0xBB));
        assert!(g.mayhem_mode_enabled);
        assert_eq!(g.fee_basis_points, 95);
    }

    #[test]
    fn fee_recipients_for_curve_routes_by_mayhem_flag() {
        let data = encoded_global();
        let r = PumpfunFeeRecipients::from_account_data(&data).expect("parse");
        assert_eq!(r.regular, pk(0xAA));
        assert_eq!(r.mayhem, pk(0xBB));
        assert_eq!(r.for_curve(false), pk(0xAA));
        assert_eq!(r.for_curve(true), pk(0xBB));
    }

    #[test]
    fn from_account_data_returns_none_on_short_input() {
        assert!(PumpfunGlobal::from_account_data(&[0u8; 4]).is_err());
        assert!(PumpfunGlobal::from_account_data(&[]).is_err());
    }
}
