//! Pumpfun `FeeConfig` account (owned by the `pump_fees` program).
//!
//! The FeeConfig PDA at [`FEE_CONFIG_PDA`](super::constants::FEE_CONFIG_PDA)
//! holds **dynamic fee tiers** that scale with a bonding curve's current
//! market cap. Since the cashback upgrade this is the source of truth —
//! the legacy hardcoded `PROTOCOL_FEE_BPS` / `CREATOR_FEE_BPS` constants
//! are only correct for the **smallest** tier.
//!
//! To price a trade accurately:
//! 1. Parse the curve (see [`BondingCurve`](super::state::BondingCurve))
//!    for `virtual_sol_reserves`, `virtual_token_reserves`,
//!    `token_total_supply`.
//! 2. Compute market cap via [`bonding_curve_market_cap`].
//! 3. Look up fees via [`calculate_fee_tier`].
//! 4. Use the returned [`PumpfunFees`] in swap math.

use borsh::BorshDeserialize;
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::OnchainState;

/// Fee rates for a given tier (basis points out of
/// [`FEE_DENOMINATOR`](super::constants::FEE_DENOMINATOR) = 10000).
#[derive(BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PumpfunFees {
    pub lp_fee_bps: u64,
    pub protocol_fee_bps: u64,
    pub creator_fee_bps: u64,
}

/// A fee tier — applies when the bonding curve's market cap is
/// `>= market_cap_lamports_threshold`. Tiers are stored sorted
/// ascending by threshold.
#[derive(BorshDeserialize, Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PumpfunFeeTier {
    pub market_cap_lamports_threshold: u128,
    pub fees: PumpfunFees,
}

/// On-chain `FeeConfig` account owned by the `pump_fees` program.
///
/// The discriminator is checked here even though it does not disambiguate the
/// account on its own — pumpfun's and pumpswap's `FeeConfig` PDAs share an owner
/// *and* a discriminator, which is why the registry routes this one by PDA. It
/// still rules out a foreign account arriving on this path, and identity checks
/// belong inside the decode: `from_account_data` is public, and "the registry
/// validated upstream" has already been false once.
#[derive(BorshDeserialize, Clone, Debug, serde::Serialize, OnchainState)]
#[idl(program = "pump_fees", account = "FeeConfig")]
#[state(discriminator = super::constants::FEE_CONFIG_DISCRIMINATOR)]
#[state(fixtures("pump_fees/fee_config.json"))]
pub struct PumpfunFeeConfig {
    pub bump: u8,
    pub admin: Pubkey,
    pub flat_fees: PumpfunFees,
    pub fee_tiers: Vec<PumpfunFeeTier>,
}

impl PumpfunFeeConfig {
    /// Parse from raw account data (8-byte Anchor discriminator prefix
    /// included).
    ///
    /// # Errors
    ///
    /// The discriminator does not match, or the body does not decode. This used
    /// to return `Option` and log the reason at `warn!`, which the default
    /// `solana_protocols=error` filter made unreachable — a decode failure on
    /// the account that prices every pumpfun swap was invisible by construction.
    pub fn from_account_data(
        data: &[u8],
    ) -> ::core::result::Result<Self, crate::parsing::state::AccountParseError> {
        <Self as crate::parsing::state::OnchainState>::from_account_data(data)
    }
}

/// Bonding curve market cap in lamports.
///
/// `market_cap = virtual_sol_reserves * token_total_supply / virtual_token_reserves`
///
/// This is the quantity Pumpfun's fee tier lookup is keyed on.
pub fn bonding_curve_market_cap(
    virtual_sol_reserves: u64,
    virtual_token_reserves: u64,
    token_total_supply: u64,
) -> u128 {
    if virtual_token_reserves == 0 {
        return 0;
    }
    (virtual_sol_reserves as u128) * (token_total_supply as u128) / (virtual_token_reserves as u128)
}

/// Look up the fee tier for a given market cap.
///
/// Tiers must be sorted ascending by threshold. Returns the fees from
/// the **highest** tier whose threshold `<= market_cap`. If `market_cap`
/// is below the first tier's threshold, returns the first tier's fees
/// (Pumpfun's convention for below-floor curves).
///
/// Returns zero fees if `fee_tiers` is empty — should never happen
/// with a live `FeeConfig` but handled defensively.
pub fn calculate_fee_tier(fee_tiers: &[PumpfunFeeTier], market_cap: u128) -> PumpfunFees {
    let Some(first) = fee_tiers.first() else {
        return PumpfunFees {
            lp_fee_bps: 0,
            protocol_fee_bps: 0,
            creator_fee_bps: 0,
        };
    };

    if market_cap < first.market_cap_lamports_threshold {
        return first.fees;
    }

    // Highest tier whose threshold <= market_cap.
    for tier in fee_tiers.iter().rev() {
        if market_cap >= tier.market_cap_lamports_threshold {
            return tier.fees;
        }
    }

    first.fees
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    /// The real pump-fees FeeConfig PDA decodes: a non-default admin and a
    /// non-empty `fee_tiers` Vec — proves the variable-length borsh layout
    /// matches the chain.
    #[test]
    fn decodes_onchain_fee_config() {
        let fx = crate::test_fixtures::AccountFixture::load("pump_fees/fee_config.json");
        let cfg = PumpfunFeeConfig::from_account_data(fx.data())
            .expect("decode real pump-fees FeeConfig");
        assert_ne!(cfg.admin, Pubkey::default(), "admin must be set");
        assert!(!cfg.fee_tiers.is_empty(), "fee_tiers must be non-empty");
    }

    fn tier(threshold: u128, proto: u64, creator: u64) -> PumpfunFeeTier {
        PumpfunFeeTier {
            market_cap_lamports_threshold: threshold,
            fees: PumpfunFees {
                lp_fee_bps: 0,
                protocol_fee_bps: proto,
                creator_fee_bps: creator,
            },
        }
    }

    #[test]
    fn market_cap_computed_from_reserves() {
        // 30 SOL virtual, 1T tokens, 1T supply → 30 SOL market cap.
        let mc = bonding_curve_market_cap(30_000_000_000, 1_000_000_000_000, 1_000_000_000_000);
        assert_eq!(mc, 30_000_000_000);
    }

    #[test]
    fn market_cap_is_zero_when_no_virtual_token_reserves() {
        assert_eq!(bonding_curve_market_cap(1, 0, 1), 0);
    }

    #[test]
    fn calculate_fee_tier_below_first_returns_first_tier() {
        let tiers = vec![tier(1000, 100, 50), tier(5000, 80, 30)];
        let fees = calculate_fee_tier(&tiers, 500);
        assert_eq!(fees.protocol_fee_bps, 100);
        assert_eq!(fees.creator_fee_bps, 50);
    }

    #[test]
    fn calculate_fee_tier_between_tiers_returns_lower_bound() {
        let tiers = vec![tier(1000, 100, 50), tier(5000, 80, 30)];
        let fees = calculate_fee_tier(&tiers, 3000);
        assert_eq!(fees.protocol_fee_bps, 100);
        assert_eq!(fees.creator_fee_bps, 50);
    }

    #[test]
    fn calculate_fee_tier_above_last_returns_last_tier() {
        let tiers = vec![tier(1000, 100, 50), tier(5000, 80, 0)];
        let fees = calculate_fee_tier(&tiers, 10000);
        assert_eq!(fees.protocol_fee_bps, 80);
        assert_eq!(fees.creator_fee_bps, 0);
    }

    #[test]
    fn calculate_fee_tier_on_empty_tiers_returns_zero_fees() {
        let fees = calculate_fee_tier(&[], 1_000);
        assert_eq!(fees.protocol_fee_bps, 0);
        assert_eq!(fees.creator_fee_bps, 0);
        assert_eq!(fees.lp_fee_bps, 0);
    }

    #[derive(BorshSerialize)]
    struct FeeConfigLayoutForEncode {
        bump: u8,
        admin: Pubkey,
        flat_fees: PumpfunFees,
        fee_tiers: Vec<PumpfunFeeTier>,
    }

    impl BorshSerialize for PumpfunFees {
        fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
            self.lp_fee_bps.serialize(writer)?;
            self.protocol_fee_bps.serialize(writer)?;
            self.creator_fee_bps.serialize(writer)?;
            Ok(())
        }
    }

    impl BorshSerialize for PumpfunFeeTier {
        fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
            self.market_cap_lamports_threshold.serialize(writer)?;
            self.fees.serialize(writer)?;
            Ok(())
        }
    }

    #[test]
    fn from_account_data_roundtrips_a_synthetic_config() {
        let encoded = {
            let cfg = FeeConfigLayoutForEncode {
                bump: 7,
                admin: Pubkey::new_from_array([0xAA; 32]),
                flat_fees: PumpfunFees {
                    lp_fee_bps: 0,
                    protocol_fee_bps: 95,
                    creator_fee_bps: 30,
                },
                fee_tiers: vec![tier(1_000_000, 100, 50), tier(10_000_000, 80, 30)],
            };
            // The real discriminator: `from_account_data` checks it now, so a
            // synthetic account built with a zero prefix is correctly refused.
            let mut out = super::super::constants::FEE_CONFIG_DISCRIMINATOR.to_vec();
            out.extend(borsh::to_vec(&cfg).unwrap());
            out
        };

        let parsed = PumpfunFeeConfig::from_account_data(&encoded).expect("parse");
        assert_eq!(parsed.bump, 7);
        assert_eq!(parsed.flat_fees.protocol_fee_bps, 95);
        assert_eq!(parsed.fee_tiers.len(), 2);
        assert_eq!(parsed.fee_tiers[0].fees.protocol_fee_bps, 100);
    }

    #[test]
    fn from_account_data_refuses_short_input() {
        assert!(PumpfunFeeConfig::from_account_data(&[0u8; 4]).is_err());
    }
}
