//! Meteora DAMM v2 `EvtSwap2` self-CPI event.
//!
//! Emitted after every successful swap via Anchor's `emit_cpi!`. The
//! event rides as an inner self-CPI whose `ix.data` is:
//!
//! ```text
//! [0..8]    ANCHOR_EVENT_DISCRIMINATOR  (outer Anchor envelope)
//! [8..16]   EVT_SWAP2_DISCRIMINATOR     (event-type tag)
//! [16..]    borsh-serialised EvtSwap2 body
//! ```

use borsh::BorshDeserialize;
use solana_program::pubkey::Pubkey;

use super::constants::{ANCHOR_EVENT_DISCRIMINATOR, EVT_SWAP2_DISCRIMINATOR};

/// User-supplied swap parameters, recorded into the event.
#[derive(BorshDeserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SwapParameters2 {
    /// Amount of input token requested by the user.
    pub amount_0: u64,
    /// Minimum acceptable output (slippage guard).
    pub amount_1: u64,
    /// `0` = ExactIn, `1` = ExactOut.
    pub swap_mode: u8,
}

/// Result of the swap, computed by the program.
#[derive(BorshDeserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SwapResult2 {
    /// Gross input (including Token-2022 transfer fees).
    pub included_fee_input_amount: u64,
    /// Net input entering the pool (post transfer fees).
    pub excluded_fee_input_amount: u64,
    /// Unconsumed input portion (partial-fill mode).
    pub amount_left: u64,
    /// Net output delivered to the user.
    pub output_amount: u64,
    /// Post-swap sqrt price (Q64.64 fixed-point).
    pub next_sqrt_price: u128,
    /// Trading fee (stays in pool for LPs).
    pub trading_fee: u64,
    /// Protocol treasury fee.
    pub protocol_fee: u64,
    /// Partner fee.
    pub partner_fee: u64,
    /// Referral fee.
    pub referral_fee: u64,
}

/// Swap event emitted by cp-amm via Anchor self-CPI.
///
/// Payload layout after the two 8-byte discriminators is pure borsh,
/// so any fields added by the program in future upgrades will trail
/// the ones we read here.
#[derive(BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct EvtSwap2 {
    /// Pool account address — the event's primary identifier.
    pub pool: Pubkey,
    /// `0` = A→B (selling token A), `1` = B→A.
    pub trade_direction: u8,
    /// `0` = BothToken fee mode, `1` = OnlyB.
    pub collect_fee_mode: u8,
    /// Whether a referral account was supplied.
    pub has_referral: bool,
    pub params: SwapParameters2,
    pub swap_result: SwapResult2,
    /// Transfer fee included in the gross input (Token-2022).
    pub included_transfer_fee_amount_in: u64,
    /// Transfer fee included in the gross output (Token-2022).
    pub included_transfer_fee_amount_out: u64,
    /// Net output to the user excluding transfer fee.
    pub excluded_transfer_fee_amount_out: u64,
    /// Unix timestamp at swap execution.
    pub current_timestamp: u64,
    /// Post-swap effective token A reserve.
    pub reserve_a_amount: u64,
    /// Post-swap effective token B reserve.
    pub reserve_b_amount: u64,
}

impl EvtSwap2 {
    /// `true` when the user sold token A (direction A→B).
    pub fn is_a_to_b(&self) -> bool {
        self.trade_direction == 0
    }

    /// Gross input (what left the user's wallet, including any
    /// Token-2022 transfer fee).
    pub fn input_amount_gross(&self) -> u64 {
        self.swap_result.included_fee_input_amount
    }

    /// Net input that entered the pool (post-transfer-fee).
    pub fn input_amount_net(&self) -> u64 {
        self.swap_result.excluded_fee_input_amount
    }

    /// Output delivered to the user (pre-transfer-fee at the AMM).
    pub fn output_amount(&self) -> u64 {
        self.swap_result.output_amount
    }

    /// Sum of every fee charged — trading + protocol + partner + referral.
    /// Denominated in whichever side `collect_fee_mode` selects.
    pub fn total_fee(&self) -> u64 {
        self.swap_result
            .trading_fee
            .saturating_add(self.swap_result.protocol_fee)
            .saturating_add(self.swap_result.partner_fee)
            .saturating_add(self.swap_result.referral_fee)
    }

    /// Parse from the full self-CPI `ix.data` (double discriminator
    /// prefix included). Returns `None` if either discriminator is
    /// wrong or the borsh body is malformed.
    pub fn from_self_cpi_data(data: &[u8]) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }
        if data[..8] != ANCHOR_EVENT_DISCRIMINATOR {
            return None;
        }
        if data[8..16] != EVT_SWAP2_DISCRIMINATOR {
            return None;
        }
        match Self::deserialize(&mut &data[16..]) {
            Ok(ev) => Some(ev),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    len = data.len(),
                    "EvtSwap2: borsh deserialization failed"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    // Re-declare every field with BorshSerialize so we can round-trip
    // synthetic events in tests. The real on-chain structs derive
    // BorshDeserialize only.
    #[derive(BorshSerialize)]
    struct SwapParameters2Ser {
        amount_0: u64,
        amount_1: u64,
        swap_mode: u8,
    }

    #[derive(BorshSerialize)]
    struct SwapResult2Ser {
        included_fee_input_amount: u64,
        excluded_fee_input_amount: u64,
        amount_left: u64,
        output_amount: u64,
        next_sqrt_price: u128,
        trading_fee: u64,
        protocol_fee: u64,
        partner_fee: u64,
        referral_fee: u64,
    }

    #[derive(BorshSerialize)]
    struct EvtSwap2Ser {
        pool: [u8; 32],
        trade_direction: u8,
        collect_fee_mode: u8,
        has_referral: bool,
        params: SwapParameters2Ser,
        swap_result: SwapResult2Ser,
        included_transfer_fee_amount_in: u64,
        included_transfer_fee_amount_out: u64,
        excluded_transfer_fee_amount_out: u64,
        current_timestamp: u64,
        reserve_a_amount: u64,
        reserve_b_amount: u64,
    }

    fn sample_bytes() -> Vec<u8> {
        let body = EvtSwap2Ser {
            pool: [0xAA; 32],
            trade_direction: 0,
            collect_fee_mode: 0,
            has_referral: false,
            params: SwapParameters2Ser {
                amount_0: 1_000_000,
                amount_1: 900_000,
                swap_mode: 0,
            },
            swap_result: SwapResult2Ser {
                included_fee_input_amount: 1_000_000,
                excluded_fee_input_amount: 990_000,
                amount_left: 0,
                output_amount: 950_000,
                next_sqrt_price: 42,
                trading_fee: 8_000,
                protocol_fee: 2_000,
                partner_fee: 0,
                referral_fee: 0,
            },
            included_transfer_fee_amount_in: 10_000,
            included_transfer_fee_amount_out: 0,
            excluded_transfer_fee_amount_out: 950_000,
            current_timestamp: 1_700_000_000,
            reserve_a_amount: 100_000_000,
            reserve_b_amount: 200_000_000,
        };
        let borsh_body = borsh::to_vec(&body).unwrap();

        let mut data = Vec::with_capacity(16 + borsh_body.len());
        data.extend_from_slice(&ANCHOR_EVENT_DISCRIMINATOR);
        data.extend_from_slice(&EVT_SWAP2_DISCRIMINATOR);
        data.extend_from_slice(&borsh_body);
        data
    }

    #[test]
    fn parses_synthetic_event_through_both_discriminators() {
        let data = sample_bytes();
        let ev = EvtSwap2::from_self_cpi_data(&data).expect("parse");
        assert_eq!(ev.pool, pk(0xAA));
        assert_eq!(ev.trade_direction, 0);
        assert!(ev.is_a_to_b());
        assert_eq!(ev.input_amount_gross(), 1_000_000);
        assert_eq!(ev.input_amount_net(), 990_000);
        assert_eq!(ev.output_amount(), 950_000);
        assert_eq!(ev.total_fee(), 10_000);
        assert_eq!(ev.reserve_a_amount, 100_000_000);
        assert_eq!(ev.reserve_b_amount, 200_000_000);
    }

    #[test]
    fn rejects_wrong_outer_discriminator() {
        let mut data = sample_bytes();
        data[..8].fill(0xFF);
        assert!(EvtSwap2::from_self_cpi_data(&data).is_none());
    }

    #[test]
    fn rejects_wrong_event_discriminator() {
        let mut data = sample_bytes();
        data[8..16].fill(0xFF);
        assert!(EvtSwap2::from_self_cpi_data(&data).is_none());
    }

    #[test]
    fn rejects_truncated_input() {
        assert!(EvtSwap2::from_self_cpi_data(&[0u8; 10]).is_none());
    }
}
