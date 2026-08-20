//! Meteora DAMM v2 swap instruction — account layout + params.
//!
//! Extraction-focused. We care about the 14-slot account layout (for
//! resolving the pool / token mints / trader) and the `amount_in /
//! minimum_amount_out` u64 pair. Builder logic is deferred until we
//! wire DAMM v2 trading.

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

/// All 14 accounts required for the DAMM v2 `swap` instruction.
///
/// Order matches `SwapCtx` in `programs/cp-amm/src/instructions/swap/ix_swap.rs`.
///
/// `#[derive(AccountMetas)]` generates the
/// [`FromAccountKeys`](crate::parsing::FromAccountKeys) impl the
/// extractor uses.
#[derive(Debug, Clone, AccountMetas)]
#[accounts(unverified = "this protocol is not modelled to the pumpfun/pumpswap standard yet; a golden fixture here would claim a verification the rest of the vertical does not have")]
pub struct SwapAccounts {
    /// 0. Pool authority PDA (seed: `b"pool_authority"`).
    #[account]
    pub pool_authority: Pubkey,
    /// 1. Pool account.
    #[account(writable)]
    pub pool: Pubkey,
    /// 2. User's source token ATA.
    #[account(writable)]
    pub input_token_account: Pubkey,
    /// 3. User's destination token ATA.
    #[account(writable)]
    pub output_token_account: Pubkey,
    /// 4. Token A vault.
    #[account(writable)]
    pub token_a_vault: Pubkey,
    /// 5. Token B vault.
    #[account(writable)]
    pub token_b_vault: Pubkey,
    /// 6. Token A mint.
    #[account]
    pub token_a_mint: Pubkey,
    /// 7. Token B mint.
    #[account]
    pub token_b_mint: Pubkey,
    /// 8. Payer / signer — the trader.
    #[account(signer)]
    pub payer: Pubkey,
    /// 9. Token A program (SPL Token or Token-2022).
    #[account]
    pub token_a_program: Pubkey,
    /// 10. Token B program.
    #[account]
    pub token_b_program: Pubkey,
    /// 11. Referral token account (program ID when unused).
    #[account(writable)]
    pub referral_token_account: Pubkey,
    /// 12. Event authority PDA (seed: `b"__event_authority"`).
    #[account]
    pub event_authority: Pubkey,
    /// 13. Program ID (Anchor self-referential CPI guard).
    #[account]
    pub program: Pubkey,
}

/// Parameters for the `swap` instruction (Borsh / LE, 16 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SwapParams {
    pub amount_in: u64,
    pub minimum_amount_out: u64,
}

impl SwapParams {
    /// Decode from the post-discriminator portion of the instruction
    /// data (16 LE bytes). Returns `None` on truncated input.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }
        let amount_in = u64::from_le_bytes(data[..8].try_into().ok()?);
        let minimum_amount_out = u64::from_le_bytes(data[8..16].try_into().ok()?);
        Some(Self {
            amount_in,
            minimum_amount_out,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_params_decodes_16_le_bytes() {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&1_234_567_u64.to_le_bytes());
        data.extend_from_slice(&9_876_543_u64.to_le_bytes());
        let p = SwapParams::from_bytes(&data).expect("decode");
        assert_eq!(p.amount_in, 1_234_567);
        assert_eq!(p.minimum_amount_out, 9_876_543);
    }

    #[test]
    fn swap_params_rejects_truncated_input() {
        assert!(SwapParams::from_bytes(&[0u8; 15]).is_none());
    }
}
