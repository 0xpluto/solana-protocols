//! Pump.fun protocol constants.
//!
//! These values are derived from the on-chain program and should be
//! updated if the protocol changes. IDL verification will catch mismatches.

use solana_program::pubkey::Pubkey;

/// Pump.fun program ID.
pub const PROGRAM_ID: Pubkey =
    solana_program::pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");

/// Pump.fun `pump_fees` program ID — owns the dynamic `FeeConfig` PDA and is
/// the `fee_program` account of buy/sell instructions.
///
/// (A previous `FEE_PROGRAM_ID` const, `CebN5W…`, was deleted 2026-08-09: probing
/// a real landed buy showed it is a fee-recipient WALLET — `Global.fee_recipients\[4\]`
/// — mistranscribed as a program id. The volume accumulators it supposedly owned
/// are PDAs of the pumpfun program itself.)
pub const PUMP_FEES_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");

/// Global configuration PDA.
pub const GLOBAL_PDA: Pubkey =
    solana_program::pubkey!("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf");

/// Fee collector account.
pub const FEE_COLLECTOR: Pubkey =
    solana_program::pubkey!("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");

/// Event authority PDA.
pub const EVENT_AUTHORITY_PDA: Pubkey =
    solana_program::pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");

/// Fee configuration PDA — `["fee_config", PUMPFUN_PROGRAM_ID]` under
/// [`PUMP_FEES_PROGRAM_ID`]. Pinned by test against the derivation and a real
/// landed buy (the previous value `ADyA8…` was stale: no account exists there).
pub const FEE_CONFIG_PDA: Pubkey =
    solana_program::pubkey!("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt");

/// Global volume accumulator PDA — `["global_volume_accumulator"]` under the
/// pumpfun program. Pinned by test against the derivation and a real landed buy
/// (the previous value `8hDKX…` was stale).
pub const GLOBAL_VOLUME_ACCUMULATOR_PDA: Pubkey =
    solana_program::pubkey!("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y");

/// `buy` instruction discriminator — `sha256("global:buy")[..8]`, derived.
pub const BUY_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("buy");

/// `buy_v2` — same semantics as `buy` (pins tokens out, ceilings SOL), newer
/// account layout. Derived.
pub const BUY_V2_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("buy_v2");

/// `buy_exact_sol_in` — the exact-IN form: pins the SOL spent, floors the
/// tokens received. Derived.
pub const BUY_EXACT_SOL_IN_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("buy_exact_sol_in");

/// `buy_exact_quote_in_v2` — exact-IN against the v2 layout. Derived.
pub const BUY_EXACT_QUOTE_IN_V2_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("buy_exact_quote_in_v2");

/// `sell_v2` — same semantics as `sell`, newer layout. Derived.
pub const SELL_V2_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("sell_v2");

/// `sell` instruction discriminator — `sha256("global:sell")[..8]`, derived.
pub const SELL_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("sell");

/// `create` instruction discriminator — `sha256("global:create")[..8]`, derived.
pub const CREATE_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("create");

/// `create_v2` instruction discriminator. First 8 bytes of
/// `SHA256("global:create_v2")`. Pumpfun moved to v2 sometime in
/// 2024; the v1 path is now rare in production traffic. The leading
/// `(name, symbol, uri)` fields are unchanged, so the same
/// [`CreateParams`](super::CreateParams) parser works on both.
///
/// [`CreateParams`]: super::instructions::CreateParams
pub const CREATE_V2_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("create_v2");

/// Bonding curve account discriminator — `sha256("account:BondingCurve")[..8]`,
/// derived at compile time (was hand-typed `[0x17, 0xb7, …]`; identical bytes).
/// `collect_creator_fee` — a creator draining their accrued fee vault.
pub const COLLECT_CREATOR_FEE_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("collect_creator_fee");

/// `collect_creator_fee_v2` — the same, settling into a token account.
pub const COLLECT_CREATOR_FEE_V2_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("collect_creator_fee_v2");

/// `distribute_creator_fees` — a vault split across a sharing config.
pub const DISTRIBUTE_CREATOR_FEES_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("distribute_creator_fees");

/// `distribute_creator_fees_v2` — the same, able to create the recipient ATA.
pub const DISTRIBUTE_CREATOR_FEES_V2_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_instruction_discriminator!("distribute_creator_fees_v2");

pub const BONDING_CURVE_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_account_discriminator!("BondingCurve");

/// Global PDA account discriminator — `sha256("account:Global")[..8]`, derived.
pub const GLOBAL_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_account_discriminator!("Global");

/// FeeConfig PDA account discriminator — `sha256("account:FeeConfig")[..8]`, derived.
pub const FEE_CONFIG_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_account_discriminator!("FeeConfig");

/// Protocol fee in basis points (0.95%).
pub const PROTOCOL_FEE_BPS: u64 = 95;

/// Creator fee in basis points (0.30%).
pub const CREATOR_FEE_BPS: u64 = 30;

/// Total fee in basis points (1.25%).
pub const TOTAL_FEE_BPS: u64 = PROTOCOL_FEE_BPS + CREATOR_FEE_BPS;

/// Fee denominator (10000 for basis points).
pub const FEE_DENOMINATOR: u64 = 10000;

/// Seed for bonding curve PDA.
pub const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";

/// Seed for creator vault PDA.
pub const CREATOR_VAULT_SEED: &[u8] = b"creator-vault";

/// Seed for the user volume accumulator PDA: `[seed, user]` under the pumpfun
/// program itself. Pinned against a real landed buy (the previous
/// `b"user-volume"` seed under the deleted `FEE_PROGRAM_ID` derived a key that
/// matches nothing on-chain).
pub const USER_VOLUME_ACCUMULATOR_SEED: &[u8] = b"user_volume_accumulator";

/// Seed for the global volume accumulator PDA (no further seeds).
pub const GLOBAL_VOLUME_SEED: &[u8] = b"global_volume_accumulator";

/// Token decimals for pump.fun tokens (always 6).
pub const TOKEN_DECIMALS: u8 = 6;

/// SOL decimals.
pub const SOL_DECIMALS: u8 = 9;

/// Initial virtual token reserves (1B tokens at 6 decimals).
pub const INITIAL_VIRTUAL_TOKEN_RESERVES: u64 = 1_000_000_000_000_000;

/// Initial virtual SOL reserves (30 SOL at 9 decimals).
pub const INITIAL_VIRTUAL_SOL_RESERVES: u64 = 30_000_000_000;

/// Required prefix length for a Pump.fun bonding curve account:
/// 8 disc + 5×u64 reserves + 1 complete + 32 creator = 81 bytes.
///
/// The 83 / 150 / 151-byte layouts observed on-chain all extend this
/// core with trailing bytes (`is_mayhem_mode`, `is_cashback_coin`, then
/// reserved padding). Dispatch is gated on the 8-byte Anchor
/// discriminator, not on any size — see [`BONDING_CURVE_DISCRIMINATOR`].
pub const BONDING_CURVE_ACCOUNT_SIZE: usize = 8 + 8 + 8 + 8 + 8 + 8 + 1 + 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_math() {
        // Verify fee percentages
        let protocol_pct = PROTOCOL_FEE_BPS as f64 / FEE_DENOMINATOR as f64 * 100.0;
        assert!((protocol_pct - 0.95).abs() < 0.001);

        let creator_pct = CREATOR_FEE_BPS as f64 / FEE_DENOMINATOR as f64 * 100.0;
        assert!((creator_pct - 0.30).abs() < 0.001);

        let total_pct = TOTAL_FEE_BPS as f64 / FEE_DENOMINATOR as f64 * 100.0;
        assert!((total_pct - 1.25).abs() < 0.001);
    }

    #[test]
    fn discriminator_lengths() {
        assert_eq!(BUY_DISCRIMINATOR.len(), 8);
        assert_eq!(SELL_DISCRIMINATOR.len(), 8);
        assert_eq!(BONDING_CURVE_DISCRIMINATOR.len(), 8);
    }

    /// The fixed-PDA constants equal their derivations, and the user-volume
    /// derivation reproduces the account of a real landed buy
    /// (`fixtures/pumpfun/ix_buy.json`, user `BwWK17…`). Reverse-engineered
    /// 2026-08-09 after a replay test caught all three stale.
    #[test]
    fn pda_constants_match_derivations() {
        let (fee_config, _) = Pubkey::find_program_address(
            &[b"fee_config", PROGRAM_ID.as_ref()],
            &PUMP_FEES_PROGRAM_ID,
        );
        assert_eq!(fee_config, FEE_CONFIG_PDA);

        let (global_vol, _) = Pubkey::find_program_address(&[GLOBAL_VOLUME_SEED], &PROGRAM_ID);
        assert_eq!(global_vol, GLOBAL_VOLUME_ACCUMULATOR_PDA);

        let user: Pubkey = "BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s"
            .parse()
            .unwrap();
        let (user_vol, _) = Pubkey::find_program_address(
            &[USER_VOLUME_ACCUMULATOR_SEED, user.as_ref()],
            &PROGRAM_ID,
        );
        assert_eq!(
            user_vol.to_string(),
            "FGFrX2q1iAjyAojjeyFDxXqdmvegjPpSWsrPmrJjeQ2f",
            "user volume accumulator derivation drifted from the on-chain value"
        );
    }

    /// Pins the compile-time-derived account discriminators to the bytes they
    /// carried when hand-typed (each independently confirmed against the Anchor
    /// derivation). If the derivation macro or an account name ever drifts, this
    /// fails — the guard the hand-typed constants never had.
    #[test]
    fn account_discriminators_match_derivation() {
        assert_eq!(
            BONDING_CURVE_DISCRIMINATOR,
            [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60]
        );
        assert_eq!(
            GLOBAL_DISCRIMINATOR,
            [0xa7, 0xe8, 0xe8, 0xb1, 0xc8, 0x6c, 0x72, 0x7f]
        );
        assert_eq!(
            FEE_CONFIG_DISCRIMINATOR,
            [0x8f, 0x34, 0x92, 0xbb, 0xdb, 0x7b, 0x4c, 0x9b]
        );
    }
}
