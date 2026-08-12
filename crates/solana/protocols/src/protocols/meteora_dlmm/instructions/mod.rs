//! Per-discriminator instruction modules.
//!
//! One file per on-chain DLMM discriminator. Each file:
//!
//! * Defines a `*Accounts` struct annotated with `#[derive(AccountMetas)]`
//!   and `#[account(writable, signer)]` flags that mirror the SDK's
//!   `instruction_with_remaining_accounts` exactly. The derive macro
//!   produces both `to_account_metas()` (build path) and
//!   `from_pubkeys()` / `FromAccountKeys::from_account_keys` (parse
//!   path) — single source of truth for both directions.
//! * Re-exports the SDK's `*_DISCRIMINATOR` const + (when the ix
//!   takes any args) its `*InstructionArgs` borsh struct.
//! * Defines a `*Ix { accounts, args }` wrapper with `parse(&ix)`,
//!   `build(accounts, args)`, and `to_instruction(&self)` helpers.
//!
//! The top-level [`MeteoraDlmmInstruction`] enum + [`parse`]
//! dispatcher pick the right module by discriminator.
//!
//! Files are emitted by `tools/gen_dlmm_ix.py` against the SDK's
//! registry source — re-run after an SDK upgrade. Do not hand-edit.

use borsh::BorshDeserialize;

use crate::parsing::{InstructionParseError, ParsedInstruction};

// ---------------------------------------------------------------------------
// Helpers shared by per-ix modules.
// ---------------------------------------------------------------------------

/// Decode `T` from the data slice after the 8-byte discriminator.
///
/// `name` is interpolated into the error message so a borsh failure
/// surfaces which ix variant tripped — handy when 71 variants share
/// the same generic decoder.
pub(crate) fn parse_anchor_args<T: BorshDeserialize>(
    data: &[u8],
    expected: &[u8; 8],
    name: &'static str,
) -> Result<T, InstructionParseError> {
    if data.len() < 8 {
        return Err(InstructionParseError::DataTooShortDetailed {
            expected: 8,
            actual: data.len(),
        });
    }
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&data[..8]);
    if &disc != expected {
        return Err(InstructionParseError::UnknownDiscriminator(disc));
    }
    T::try_from_slice(&data[8..])
        .map_err(|e| InstructionParseError::DeserializationFailed(format!("{name} args: {e}")))
}

// ---------------------------------------------------------------------------
// Per-discriminator module declarations.
// ---------------------------------------------------------------------------

pub mod add_liquidity;
pub mod add_liquidity2;
pub mod add_liquidity_by_strategy;
pub mod add_liquidity_by_strategy2;
pub mod add_liquidity_by_strategy_one_side;
pub mod add_liquidity_by_weight;
pub mod add_liquidity_one_side;
pub mod add_liquidity_one_side_precise;
pub mod add_liquidity_one_side_precise2;
pub mod claim_fee;
pub mod claim_fee2;
pub mod claim_reward;
pub mod claim_reward2;
pub mod close_claim_protocol_fee_operator;
pub mod close_position;
pub mod close_position2;
pub mod close_position_if_empty;
pub mod close_preset_parameter;
pub mod close_preset_parameter2;
pub mod close_token_badge;
pub mod create_claim_protocol_fee_operator;
pub mod decrease_position_length;
pub mod for_idl_type_generation_do_not_call;
pub mod fund_reward;
pub mod go_to_a_bin;
pub mod increase_oracle_length;
pub mod increase_position_length;
pub mod increase_position_length2;
pub mod initialize_bin_array;
pub mod initialize_bin_array_bitmap_extension;
pub mod initialize_customizable_permissionless_lb_pair;
pub mod initialize_customizable_permissionless_lb_pair2;
pub mod initialize_lb_pair;
pub mod initialize_lb_pair2;
pub mod initialize_permission_lb_pair;
pub mod initialize_position;
pub mod initialize_position2;
pub mod initialize_position_by_operator;
pub mod initialize_position_pda;
pub mod initialize_preset_parameter;
pub mod initialize_preset_parameter2;
pub mod initialize_reward;
pub mod initialize_token_badge;
pub mod migrate_bin_array;
pub mod migrate_position;
pub mod rebalance_liquidity;
pub mod remove_all_liquidity;
pub mod remove_liquidity;
pub mod remove_liquidity2;
pub mod remove_liquidity_by_range;
pub mod remove_liquidity_by_range2;
pub mod set_activation_point;
pub mod set_pair_status;
pub mod set_pair_status_permissionless;
pub mod set_pre_activation_duration;
pub mod set_pre_activation_swap_address;
pub mod swap;
pub mod swap2;
pub mod swap_exact_out;
pub mod swap_exact_out2;
pub mod swap_with_price_impact;
pub mod swap_with_price_impact2;
pub mod update_base_fee_parameters;
pub mod update_dynamic_fee_parameters;
pub mod update_fees_and_reward2;
pub mod update_fees_and_rewards;
pub mod update_position_operator;
pub mod update_reward_duration;
pub mod update_reward_funder;
pub mod withdraw_ineligible_reward;
pub mod withdraw_protocol_fee;

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Typed enum over every DLMM instruction. Variant payloads are
/// boxed because several ix carry 25+ Pubkey fields and we don't
/// want the discriminator size of the enum to be dominated by the
/// largest variant.
#[derive(Debug, Clone)]
pub enum MeteoraDlmmInstruction {
    AddLiquidity(Box<add_liquidity::AddLiquidityIx>),
    AddLiquidity2(Box<add_liquidity2::AddLiquidity2Ix>),
    AddLiquidityByStrategy(Box<add_liquidity_by_strategy::AddLiquidityByStrategyIx>),
    AddLiquidityByStrategy2(Box<add_liquidity_by_strategy2::AddLiquidityByStrategy2Ix>),
    AddLiquidityByStrategyOneSide(
        Box<add_liquidity_by_strategy_one_side::AddLiquidityByStrategyOneSideIx>,
    ),
    AddLiquidityByWeight(Box<add_liquidity_by_weight::AddLiquidityByWeightIx>),
    AddLiquidityOneSide(Box<add_liquidity_one_side::AddLiquidityOneSideIx>),
    AddLiquidityOneSidePrecise(Box<add_liquidity_one_side_precise::AddLiquidityOneSidePreciseIx>),
    AddLiquidityOneSidePrecise2(
        Box<add_liquidity_one_side_precise2::AddLiquidityOneSidePrecise2Ix>,
    ),
    ClaimFee(Box<claim_fee::ClaimFeeIx>),
    ClaimFee2(Box<claim_fee2::ClaimFee2Ix>),
    ClaimReward(Box<claim_reward::ClaimRewardIx>),
    ClaimReward2(Box<claim_reward2::ClaimReward2Ix>),
    CloseClaimProtocolFeeOperator(
        Box<close_claim_protocol_fee_operator::CloseClaimProtocolFeeOperatorIx>,
    ),
    ClosePosition(Box<close_position::ClosePositionIx>),
    ClosePosition2(Box<close_position2::ClosePosition2Ix>),
    ClosePositionIfEmpty(Box<close_position_if_empty::ClosePositionIfEmptyIx>),
    ClosePresetParameter(Box<close_preset_parameter::ClosePresetParameterIx>),
    ClosePresetParameter2(Box<close_preset_parameter2::ClosePresetParameter2Ix>),
    CloseTokenBadge(Box<close_token_badge::CloseTokenBadgeIx>),
    CreateClaimProtocolFeeOperator(
        Box<create_claim_protocol_fee_operator::CreateClaimProtocolFeeOperatorIx>,
    ),
    DecreasePositionLength(Box<decrease_position_length::DecreasePositionLengthIx>),
    ForIdlTypeGenerationDoNotCall(
        Box<for_idl_type_generation_do_not_call::ForIdlTypeGenerationDoNotCallIx>,
    ),
    FundReward(Box<fund_reward::FundRewardIx>),
    GoToABin(Box<go_to_a_bin::GoToABinIx>),
    IncreaseOracleLength(Box<increase_oracle_length::IncreaseOracleLengthIx>),
    IncreasePositionLength(Box<increase_position_length::IncreasePositionLengthIx>),
    IncreasePositionLength2(Box<increase_position_length2::IncreasePositionLength2Ix>),
    InitializeBinArray(Box<initialize_bin_array::InitializeBinArrayIx>),
    InitializeBinArrayBitmapExtension(
        Box<initialize_bin_array_bitmap_extension::InitializeBinArrayBitmapExtensionIx>,
    ),
    InitializeCustomizablePermissionlessLbPair(
        Box<initialize_customizable_permissionless_lb_pair::InitializeCustomizablePermissionlessLbPairIx>,
    ),
    InitializeCustomizablePermissionlessLbPair2(
        Box<initialize_customizable_permissionless_lb_pair2::InitializeCustomizablePermissionlessLbPair2Ix>,
    ),
    InitializeLbPair(Box<initialize_lb_pair::InitializeLbPairIx>),
    InitializeLbPair2(Box<initialize_lb_pair2::InitializeLbPair2Ix>),
    InitializePermissionLbPair(Box<initialize_permission_lb_pair::InitializePermissionLbPairIx>),
    InitializePosition(Box<initialize_position::InitializePositionIx>),
    InitializePosition2(Box<initialize_position2::InitializePosition2Ix>),
    InitializePositionByOperator(
        Box<initialize_position_by_operator::InitializePositionByOperatorIx>,
    ),
    InitializePositionPda(Box<initialize_position_pda::InitializePositionPdaIx>),
    InitializePresetParameter(Box<initialize_preset_parameter::InitializePresetParameterIx>),
    InitializePresetParameter2(Box<initialize_preset_parameter2::InitializePresetParameter2Ix>),
    InitializeReward(Box<initialize_reward::InitializeRewardIx>),
    InitializeTokenBadge(Box<initialize_token_badge::InitializeTokenBadgeIx>),
    MigrateBinArray(Box<migrate_bin_array::MigrateBinArrayIx>),
    MigratePosition(Box<migrate_position::MigratePositionIx>),
    RebalanceLiquidity(Box<rebalance_liquidity::RebalanceLiquidityIx>),
    RemoveAllLiquidity(Box<remove_all_liquidity::RemoveAllLiquidityIx>),
    RemoveLiquidity(Box<remove_liquidity::RemoveLiquidityIx>),
    RemoveLiquidity2(Box<remove_liquidity2::RemoveLiquidity2Ix>),
    RemoveLiquidityByRange(Box<remove_liquidity_by_range::RemoveLiquidityByRangeIx>),
    RemoveLiquidityByRange2(Box<remove_liquidity_by_range2::RemoveLiquidityByRange2Ix>),
    SetActivationPoint(Box<set_activation_point::SetActivationPointIx>),
    SetPairStatus(Box<set_pair_status::SetPairStatusIx>),
    SetPairStatusPermissionless(Box<set_pair_status_permissionless::SetPairStatusPermissionlessIx>),
    SetPreActivationDuration(Box<set_pre_activation_duration::SetPreActivationDurationIx>),
    SetPreActivationSwapAddress(Box<set_pre_activation_swap_address::SetPreActivationSwapAddressIx>),
    Swap(Box<swap::SwapIx>),
    Swap2(Box<swap2::Swap2Ix>),
    SwapExactOut(Box<swap_exact_out::SwapExactOutIx>),
    SwapExactOut2(Box<swap_exact_out2::SwapExactOut2Ix>),
    SwapWithPriceImpact(Box<swap_with_price_impact::SwapWithPriceImpactIx>),
    SwapWithPriceImpact2(Box<swap_with_price_impact2::SwapWithPriceImpact2Ix>),
    UpdateBaseFeeParameters(Box<update_base_fee_parameters::UpdateBaseFeeParametersIx>),
    UpdateDynamicFeeParameters(Box<update_dynamic_fee_parameters::UpdateDynamicFeeParametersIx>),
    UpdateFeesAndReward2(Box<update_fees_and_reward2::UpdateFeesAndReward2Ix>),
    UpdateFeesAndRewards(Box<update_fees_and_rewards::UpdateFeesAndRewardsIx>),
    UpdatePositionOperator(Box<update_position_operator::UpdatePositionOperatorIx>),
    UpdateRewardDuration(Box<update_reward_duration::UpdateRewardDurationIx>),
    UpdateRewardFunder(Box<update_reward_funder::UpdateRewardFunderIx>),
    WithdrawIneligibleReward(Box<withdraw_ineligible_reward::WithdrawIneligibleRewardIx>),
    WithdrawProtocolFee(Box<withdraw_protocol_fee::WithdrawProtocolFeeIx>),
}

/// Resolve a [`ParsedInstruction`] from the DLMM program into the
/// typed enum. Returns
/// [`InstructionParseError::UnknownDiscriminator`] if the 8-byte
/// prefix doesn't match any known DLMM ix.
pub fn parse(ix: &ParsedInstruction) -> Result<MeteoraDlmmInstruction, InstructionParseError> {
    if ix.data.len() < 8 {
        return Err(InstructionParseError::DataTooShortDetailed {
            expected: 8,
            actual: ix.data.len(),
        });
    }
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&ix.data[..8]);
    dispatch(&disc, ix)
}

include!("_dispatch.rs");

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::ParsedInstructionBuilder;
    use solana_program::pubkey::Pubkey;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    fn build_ix(disc: [u8; 8], extra_data: &[u8], n_accounts: usize) -> ParsedInstruction {
        let mut data = disc.to_vec();
        data.extend_from_slice(extra_data);
        let accounts: Vec<Pubkey> = (0..n_accounts).map(|i| pk(i as u8 + 1)).collect();
        ParsedInstructionBuilder::new()
            .program_id(super::super::PROGRAM_ID)
            .accounts(accounts)
            .data(data)
            .build()
    }

    #[test]
    fn parse_swap_dispatch() {
        // Swap args: amount_in (u64) + min_amount_out (u64) = 16 bytes.
        let ix = build_ix(swap::SWAP_DISCRIMINATOR, &[0u8; 16], 15);
        match parse(&ix).expect("dispatch") {
            MeteoraDlmmInstruction::Swap(_) => {}
            other => panic!("expected Swap, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn parse_claim_fee_dispatch() {
        let ix = build_ix(claim_fee::CLAIM_FEE_DISCRIMINATOR, &[], 14);
        assert!(matches!(
            parse(&ix),
            Ok(MeteoraDlmmInstruction::ClaimFee(_))
        ));
    }

    #[test]
    fn parse_add_liquidity_dispatch() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0u8; 16]); // total_amount_x + total_amount_y
        data.extend_from_slice(&0u32.to_le_bytes()); // empty bin distribution vec
        let ix = build_ix(add_liquidity::ADD_LIQUIDITY_DISCRIMINATOR, &data, 16);
        assert!(matches!(
            parse(&ix),
            Ok(MeteoraDlmmInstruction::AddLiquidity(_))
        ));
    }

    #[test]
    fn unknown_discriminator_returns_error() {
        let ix = build_ix([0xFF; 8], &[0u8; 16], 15);
        assert!(matches!(
            parse(&ix),
            Err(InstructionParseError::UnknownDiscriminator(_))
        ));
    }

    #[test]
    fn data_too_short_returns_error() {
        let ix = ParsedInstructionBuilder::new()
            .program_id(super::super::PROGRAM_ID)
            .accounts(vec![pk(1); 10])
            .data(vec![1, 2, 3])
            .build();
        assert!(matches!(
            parse(&ix),
            Err(InstructionParseError::DataTooShortDetailed { .. })
        ));
    }

    /// Round-trip: build a swap instruction, then parse it back into
    /// the same typed struct. Validates that the macro-generated
    /// `to_account_metas` and `from_pubkeys` agree on field order
    /// and writability flags.
    #[test]
    fn swap_roundtrip_build_then_parse() {
        let accounts = swap::SwapAccounts {
            lb_pair: pk(1),
            bin_array_bitmap_extension: super::super::PROGRAM_ID, // sentinel = absent
            reserve_x: pk(3),
            reserve_y: pk(4),
            user_token_in: pk(5),
            user_token_out: pk(6),
            token_x_mint: pk(7),
            token_y_mint: pk(8),
            oracle: pk(9),
            host_fee_in: super::super::PROGRAM_ID, // sentinel = absent
            user: pk(11),
            token_x_program: pk(12),
            token_y_program: pk(13),
            event_authority: pk(14),
            program: pk(15),
        };
        let args = swap::SwapInstructionArgs {
            amount_in: 1_000_000_000,
            min_amount_out: 950_000_000,
        };
        let ix = swap::SwapIx::build(accounts, args);

        // Re-parse from the freshly built instruction.
        let parsed_ix = ParsedInstructionBuilder::new()
            .program_id(ix.program_id)
            .accounts(ix.accounts.iter().map(|m| m.pubkey).collect())
            .data(ix.data.clone())
            .build();
        let decoded = match parse(&parsed_ix).expect("dispatch") {
            MeteoraDlmmInstruction::Swap(b) => b,
            other => panic!("wrong variant: {:?}", std::mem::discriminant(&other)),
        };
        assert_eq!(decoded.accounts.lb_pair, pk(1));
        assert_eq!(decoded.accounts.user, pk(11));
        assert_eq!(decoded.args.amount_in, 1_000_000_000);
        assert_eq!(decoded.args.min_amount_out, 950_000_000);
    }
}
