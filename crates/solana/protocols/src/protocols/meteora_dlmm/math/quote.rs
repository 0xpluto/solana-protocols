//! Top-level swap quoter.
//!
//! Walks the bin space crossing bins until the user-supplied amount
//! is satisfied (exact-in) or the requested output is delivered
//! (exact-out). Mirrors the on-chain DLMM program's swap loop.
//!
//! Inputs:
//! * `lb_pair` — pool snapshot. Cloned internally; the caller's
//!   reference isn't mutated.
//! * `bin_arrays` — a `HashMap<Pubkey, BinArray>` keyed by bin-array
//!   PDA. Must contain whichever arrays the swap traverses; missing
//!   arrays surface as `Err("Active bin array not found in cache")`.
//! * `bitmap_extension` — optional. Required only for sparse pools
//!   whose active bin range exits the inline bitmap.
//! * `current_timestamp` — used to decay volatility references; pass
//!   the current unix time (or a fixed value if you want to disable
//!   decay for deterministic testing — the on-chain code reads
//!   `Clock::get()`).
//!
//! Outputs are returned as plain structs — `(amount_out, fee)` for
//! exact-in, `(amount_in, fee)` for exact-out.

use std::collections::HashMap;

use solana_program::pubkey::Pubkey;

use super::super::accounts::derive_bin_array_address;
use super::super::state::{BinArray, BinArrayBitmapExtension, LbPair, LbPairExt};
use super::bin::{
    bin_swap, bin_swap_exact_out, get_amount_in, get_or_store_bin_price, is_empty, BinSwapResult,
};
use super::bin_array::{bin_id_to_bin_array_index, get_bin_mut, is_bin_id_within_range};
use super::bitmap::{
    is_overflow_default_bin_array_bitmap, next_bin_array_index_with_liquidity_extension,
    next_bin_array_index_with_liquidity_internal,
};
use super::lb_pair::{
    advance_active_bin, compute_fee, update_references, update_volatility_accumulator,
};

/// Result of an exact-in quote.
#[derive(Debug, Clone, Copy)]
pub struct ExactInQuote {
    pub amount_out: u64,
    pub fee: u64,
}

/// Result of an exact-out quote.
#[derive(Debug, Clone, Copy)]
pub struct ExactOutQuote {
    pub amount_in: u64,
    pub fee: u64,
}

/// Validate that the pool is in a state we can quote against. Mirrors
/// the on-chain `validate_swap_activation`. Pre-activation check is
/// best-effort — we don't pull `ActivationType` from the SDK enum
/// space, so we treat unknown activation types as "permissionless"
/// (i.e. always allowed) which matches the most common case.
fn validate_swap_activation(
    pair: &LbPair,
    current_timestamp: u64,
    current_slot: u64,
) -> Result<(), &'static str> {
    if !pair.is_active() {
        return Err("Pool is disabled");
    }
    // Permission pools (pair_type = 1) gate on activation_point.
    if pair.pair_type == 1 {
        let current_point = match pair.activation_type {
            // 0 = Slot, 1 = Timestamp; anything else, treat as
            // already-activated rather than blocking.
            0 => current_slot,
            1 => current_timestamp,
            _ => return Ok(()),
        };
        if current_point < pair.activation_point {
            return Err("Pool not yet activated");
        }
    }
    Ok(())
}

/// Resolve up to `take_count` bin-array PDAs that the swap will
/// traverse, in order. The on-chain program needs these as
/// `remaining_accounts`; offline this drives the bin-walk loop.
pub fn get_bin_array_pubkeys_for_swap(
    lb_pair_pubkey: Pubkey,
    pair: &LbPair,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
    swap_for_y: bool,
    take_count: u8,
) -> Result<Vec<Pubkey>, &'static str> {
    let mut start_bin_array_idx = bin_id_to_bin_array_index(pair.active_id)?;
    let mut indices: Vec<i32> = Vec::new();
    let increment = if swap_for_y { -1 } else { 1 };

    loop {
        if indices.len() == take_count as usize {
            break;
        }
        if is_overflow_default_bin_array_bitmap(start_bin_array_idx) {
            let Some(ext) = bitmap_extension else {
                break;
            };
            let Ok((next, has_liq)) =
                next_bin_array_index_with_liquidity_extension(ext, swap_for_y, start_bin_array_idx)
            else {
                break;
            };
            if has_liq {
                indices.push(next);
                start_bin_array_idx = next + increment;
            } else {
                start_bin_array_idx = next;
            }
        } else {
            let Ok((next, has_liq)) =
                next_bin_array_index_with_liquidity_internal(pair, swap_for_y, start_bin_array_idx)
            else {
                break;
            };
            if has_liq {
                indices.push(next);
                start_bin_array_idx = next + increment;
            } else {
                start_bin_array_idx = next;
            }
        }
    }

    Ok(indices
        .into_iter()
        .map(|idx| derive_bin_array_address(&lb_pair_pubkey, idx.into()))
        .collect())
}

/// Quote a swap that consumes `amount_in` of the inbound side and
/// returns `(amount_out, fee)`.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    lb_pair_pubkey: Pubkey,
    pair: &LbPair,
    mut amount_in: u64,
    swap_for_y: bool,
    bin_arrays: HashMap<Pubkey, BinArray>,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
    current_timestamp: u64,
    current_slot: u64,
) -> Result<ExactInQuote, &'static str> {
    validate_swap_activation(pair, current_timestamp, current_slot)?;

    let mut pair = pair.clone();
    update_references(&mut pair, current_timestamp as i64)?;

    let mut total_amount_out: u64 = 0;
    let mut total_fee: u64 = 0;

    while amount_in > 0 {
        let active_pda =
            get_bin_array_pubkeys_for_swap(lb_pair_pubkey, &pair, bitmap_extension, swap_for_y, 1)?
                .pop()
                .ok_or("Pool out of liquidity")?;

        let mut active_array = bin_arrays
            .get(&active_pda)
            .cloned()
            .ok_or("Active bin array not found in cache")?;

        loop {
            if !is_bin_id_within_range(&active_array, pair.active_id) || amount_in == 0 {
                break;
            }
            update_volatility_accumulator(&mut pair)?;
            let bin_step = pair.bin_step;
            let active_id = pair.active_id;
            let active_bin = get_bin_mut(&mut active_array, active_id)?;
            let price = get_or_store_bin_price(active_bin, active_id, bin_step)?;

            // !swap_for_y means "looking for token X liquidity"; if
            // the bin is empty on that side we just skip and advance.
            if !is_empty(active_bin, !swap_for_y) {
                let BinSwapResult {
                    amount_in_with_fees,
                    amount_out,
                    fee,
                    ..
                } = bin_swap(active_bin, amount_in, price, swap_for_y, &pair, None)?;
                amount_in = amount_in
                    .checked_sub(amount_in_with_fees)
                    .ok_or("MathOverflow")?;
                total_amount_out = total_amount_out
                    .checked_add(amount_out)
                    .ok_or("MathOverflow")?;
                total_fee = total_fee.checked_add(fee).ok_or("MathOverflow")?;
            }
            if amount_in > 0 {
                advance_active_bin(&mut pair, swap_for_y)?;
            }
        }
    }

    Ok(ExactInQuote {
        amount_out: total_amount_out,
        fee: total_fee,
    })
}

/// Quote a swap that delivers exactly `amount_out` and returns
/// `(amount_in, fee)`. The on-chain program ceil-rounds inputs so
/// the quoted `amount_in` is the *minimum* the user must supply.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_out(
    lb_pair_pubkey: Pubkey,
    pair: &LbPair,
    mut amount_out: u64,
    swap_for_y: bool,
    bin_arrays: HashMap<Pubkey, BinArray>,
    bitmap_extension: Option<&BinArrayBitmapExtension>,
    current_timestamp: u64,
    current_slot: u64,
) -> Result<ExactOutQuote, &'static str> {
    validate_swap_activation(pair, current_timestamp, current_slot)?;

    let mut pair = pair.clone();
    update_references(&mut pair, current_timestamp as i64)?;

    let mut total_amount_in: u64 = 0;
    let mut total_fee: u64 = 0;

    while amount_out > 0 {
        let active_pda =
            get_bin_array_pubkeys_for_swap(lb_pair_pubkey, &pair, bitmap_extension, swap_for_y, 1)?
                .pop()
                .ok_or("Pool out of liquidity")?;

        let mut active_array = bin_arrays
            .get(&active_pda)
            .cloned()
            .ok_or("Active bin array not found in cache")?;

        loop {
            if !is_bin_id_within_range(&active_array, pair.active_id) || amount_out == 0 {
                break;
            }
            update_volatility_accumulator(&mut pair)?;
            let bin_step = pair.bin_step;
            let active_id = pair.active_id;
            let active_bin = get_bin_mut(&mut active_array, active_id)?;
            let price = get_or_store_bin_price(active_bin, active_id, bin_step)?;

            if !is_empty(active_bin, !swap_for_y) {
                let bin_max_out = super::bin::max_amount_out(active_bin, swap_for_y);
                if amount_out >= bin_max_out {
                    let max_in = super::bin::max_amount_in(active_bin, price, swap_for_y)?;
                    let max_fee = compute_fee(&pair, max_in)?;
                    total_amount_in = total_amount_in.checked_add(max_in).ok_or("MathOverflow")?;
                    total_fee = total_fee.checked_add(max_fee).ok_or("MathOverflow")?;
                    amount_out = amount_out.checked_sub(bin_max_out).ok_or("MathOverflow")?;
                } else {
                    let single_in = get_amount_in(amount_out, price, swap_for_y)?;
                    let fee = compute_fee(&pair, single_in)?;
                    total_amount_in = total_amount_in
                        .checked_add(single_in)
                        .ok_or("MathOverflow")?;
                    total_fee = total_fee.checked_add(fee).ok_or("MathOverflow")?;
                    // Run the bin update via bin_swap_exact_out so
                    // the bin's reserves stay consistent with the
                    // amount_out we're claiming. The result's
                    // amount_in_with_fees would equal single_in + fee
                    // by construction.
                    let _ = bin_swap_exact_out(
                        active_bin,
                        single_in.checked_add(fee).ok_or("MathOverflow")?,
                        price,
                        swap_for_y,
                        &pair,
                        None,
                        amount_out,
                    )?;
                    amount_out = 0;
                }
            }
            if amount_out > 0 {
                advance_active_bin(&mut pair, swap_for_y)?;
            }
        }
    }

    let gross_amount_in = total_amount_in
        .checked_add(total_fee)
        .ok_or("MathOverflow")?;
    Ok(ExactOutQuote {
        amount_in: gross_amount_in,
        fee: total_fee,
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::state::{Bin, BinArray, LbPair};
    use super::*;
    use meteora_dlmm_sdk::types::{ProtocolFee, RewardInfo, StaticParameters, VariableParameters};
    use solana_program::pubkey::Pubkey as ProgramPubkey;

    fn pk(b: u8) -> ProgramPubkey {
        ProgramPubkey::new_from_array([b; 32])
    }

    /// SDK `Pubkey` literal. The SDK's pubkey type is from
    /// `solana-pubkey 4.x`; we round-trip via bytes.
    fn sdk_pk(b: u8) -> sdk_solana_pubkey::Pubkey {
        sdk_solana_pubkey::Pubkey::new_from_array([b; 32])
    }

    /// Build an `LbPair` with controllable fee / volatility params.
    /// Sets the inline bitmap so bin-array index 0 is flagged
    /// (covering bins 0..=69) — that's where we'll place liquidity.
    fn make_test_pair(
        active_id: i32,
        bin_step: u16,
        base_factor: u16,
        variable_fee_control: u32,
        volatility_accumulator: u32,
    ) -> LbPair {
        let mut bin_array_bitmap = [0u64; 16];
        // Inline bitmap covers indices [-512, 511]. Word 8 / bit 0 ==
        // index 0 (since `index + BIN_ARRAY_BITMAP_SIZE == 512`).
        bin_array_bitmap[8] = 1;

        LbPair {
            discriminator: [0u8; 8],
            parameters: StaticParameters {
                base_factor,
                filter_period: 30,
                decay_period: 600,
                reduction_factor: 5_000,
                variable_fee_control,
                max_volatility_accumulator: 350_000,
                min_bin_id: -443_636,
                max_bin_id: 443_636,
                protocol_share: 0,
                base_fee_power_factor: 0,
                padding: [0u8; 5],
            },
            v_parameters: VariableParameters {
                volatility_accumulator,
                volatility_reference: volatility_accumulator,
                index_reference: active_id,
                padding: [0u8; 4],
                last_update_timestamp: 1_700_000_000,
                padding1: [0u8; 8],
            },
            bump_seed: [0u8; 1],
            bin_step_seed: [0u8; 2],
            pair_type: 0,
            active_id,
            bin_step,
            status: 0, // Enabled
            require_base_factor_seed: 0,
            base_factor_seed: [0u8; 2],
            activation_type: 0,
            creator_pool_on_off_control: 0,
            token_x_mint: sdk_pk(1),
            token_y_mint: sdk_pk(2),
            reserve_x: sdk_pk(3),
            reserve_y: sdk_pk(4),
            protocol_fee: ProtocolFee {
                amount_x: 0,
                amount_y: 0,
            },
            padding1: [0u8; 32],
            reward_infos: [
                RewardInfo {
                    mint: sdk_pk(0),
                    vault: sdk_pk(0),
                    funder: sdk_pk(0),
                    reward_duration: 0,
                    reward_duration_end: 0,
                    reward_rate: 0,
                    last_update_time: 0,
                    cumulative_seconds_with_empty_liquidity_reward: 0,
                },
                RewardInfo {
                    mint: sdk_pk(0),
                    vault: sdk_pk(0),
                    funder: sdk_pk(0),
                    reward_duration: 0,
                    reward_duration_end: 0,
                    reward_rate: 0,
                    last_update_time: 0,
                    cumulative_seconds_with_empty_liquidity_reward: 0,
                },
            ],
            oracle: sdk_pk(0),
            bin_array_bitmap,
            last_updated_at: 1_700_000_000,
            padding2: [0u8; 32],
            pre_activation_swap_address: sdk_pk(0),
            base_key: sdk_pk(0),
            activation_point: 0,
            pre_activation_duration: 0,
            padding3: [0u8; 8],
            padding4: 0,
            creator: sdk_pk(0),
            token_mint_x_program_flag: 0,
            token_mint_y_program_flag: 0,
            reserved: [0u8; 22],
        }
    }

    /// Build a `BinArray` with a single bin of liquidity at the
    /// requested offset. `(amount_x, amount_y)` are raw token units.
    fn make_test_bin_array(
        lb_pair: ProgramPubkey,
        index: i64,
        bin_offset: usize,
        amount_x: u64,
        amount_y: u64,
    ) -> BinArray {
        let default_bin = Bin {
            amount_x: 0,
            amount_y: 0,
            price: 0,
            liquidity_supply: 0,
            reward_per_token_stored: [0u128; 2],
            fee_amount_x_per_token_stored: 0,
            fee_amount_y_per_token_stored: 0,
            amount_x_in: 0,
            amount_y_in: 0,
        };
        let mut bins: [Bin; 70] = std::array::from_fn(|_| default_bin.clone());
        bins[bin_offset] = Bin {
            amount_x,
            amount_y,
            price: 0,                      // Lazily computed by the bin walk.
            liquidity_supply: 1u128 << 64, // 1.0 in Q64.64.
            ..default_bin
        };
        BinArray {
            discriminator: [0u8; 8],
            index,
            version: 0,
            padding: [0u8; 7],
            lb_pair: sdk_solana_pubkey::Pubkey::new_from_array(lb_pair.to_bytes()),
            bins,
        }
    }

    #[test]
    fn quote_exact_in_single_bin() {
        let pool = pk(0xAA);
        let active_id: i32 = 5;
        let pair = make_test_pair(active_id, 10, 10, 0, 0); // 0.1% bin step, no variable fee

        // Bin array index 0 covers bins 0..=69. Put 1 token-Y at bin 5.
        let array = make_test_bin_array(pool, 0, active_id as usize, 0, 1_000_000_000);
        let array_pda = derive_bin_array_address(&pool, 0);
        let mut bin_arrays = HashMap::new();
        bin_arrays.insert(array_pda, array);

        // Swap X → Y, 1M units in.
        let result = quote_exact_in(
            pool,
            &pair,
            1_000_000,
            true, // swap_for_y
            bin_arrays,
            None,
            1_700_000_000,
            0,
        )
        .expect("quote should succeed");

        assert!(result.amount_out > 0, "should produce some output");
        assert!(result.fee > 0, "should charge fee on a non-trivial swap");
    }

    #[test]
    fn disabled_pool_returns_error() {
        let pool = pk(0xAA);
        let mut pair = make_test_pair(5, 10, 10, 0, 0);
        pair.status = 1; // Disabled

        let result = quote_exact_in(pool, &pair, 100, true, HashMap::new(), None, 0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn missing_bin_array_returns_error() {
        let pool = pk(0xAA);
        let pair = make_test_pair(5, 10, 10, 0, 0);

        // Empty cache — quote should fail rather than panic.
        let result = quote_exact_in(
            pool,
            &pair,
            1_000_000,
            true,
            HashMap::new(),
            None,
            1_700_000_000,
            0,
        );
        assert!(result.is_err());
    }
}
