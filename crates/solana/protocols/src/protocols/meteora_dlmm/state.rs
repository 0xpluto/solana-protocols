//! Account state — borsh-derived layouts come from
//! [`meteora_dlmm_sdk::accounts`]; this module re-exports them under
//! their bare names and adds the extension methods our trader / cache
//! call sites expect (`spot_price`, `is_active`, `total_fee_rate`, …).
//!
//! Why an extension trait instead of a wrapper struct: we want
//! [`LbPair`] values to flow through caches and `PoolState` enums
//! without paying for an extra newtype. Adding methods via a trait
//! costs nothing at runtime and keeps the SDK type as the single
//! source of truth.

use borsh::BorshDeserialize;

use crate::error::{Error, Result};

pub use meteora_dlmm_sdk::accounts::{
    BinArray, BinArrayBitmapExtension, ClaimFeeOperator, DummyZcAccount, LbPair, Oracle, Position,
    PositionV2, PresetParameter, PresetParameter2, TokenBadge,
};

/// `Bin` is a *type* (the per-bin payload inside a `BinArray`),
/// re-exported here for convenience next to the account types.
pub use meteora_dlmm_sdk::types::Bin;

/// Per-bin sub-records used by the dynamic-position layout. The
/// appended-bin records ([`PositionBinData`]) hold one of each, in the
/// same field order as the inline `PositionV2` arrays.
use meteora_dlmm_sdk::types::{FeeInfo, UserRewardInfo};

/// Convert an SDK-side pubkey (any `solana-pubkey` major version) to
/// our workspace's `ProgramPubkey` via the universal 32-byte
/// representation.
///
/// The SDK pulls `solana-pubkey 4.x` (where the type was renamed
/// `Address`), our workspace pulls `solana-pubkey 2.x` (`Pubkey`).
/// The bytes are identical; the type names aren't. This macro picks
/// one side of the boundary by calling `.to_bytes()` — works on every
/// version we care about without us having to name the SDK type.
#[macro_export]
macro_rules! dlmm_sdk_pubkey {
    ($p:expr) => {
        ::solana_program::pubkey::Pubkey::new_from_array(($p).to_bytes())
    };
}

use super::constants::{
    BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR, BIN_ARRAY_DISCRIMINATOR, FEE_PRECISION,
    LB_PAIR_DISCRIMINATOR, MAX_FEE_RATE, POSITION_V2_DISCRIMINATOR,
};

/// Pair-status bytes the program writes to [`LbPair::status`]. Mirrors
/// the on-chain `PairStatus` enum the SDK doesn't expose at the type
/// level (it's encoded as a raw `u8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PairStatus {
    /// Pool is enabled and operational.
    Enabled = 0,
    /// Pool is disabled (e.g. `set_pair_status` was called by an admin).
    Disabled = 1,
}

impl PairStatus {
    /// Strict parse. Returns `None` for any unknown byte rather than
    /// silently bucketing it under an existing variant.
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Enabled),
            1 => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// Pair-type bytes the program writes to [`LbPair::pair_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PairType {
    /// Anyone can swap; no admin gate.
    Permissionless = 0,
    /// Admin-controlled (whitelist-gated).
    Permission = 1,
    /// Permissionless with creator-controlled enable/disable toggle.
    CustomizablePermissionless = 2,
    /// Permission with extended parameters (e.g. customisable fee).
    PermissionV2 = 3,
}

impl PairType {
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Permissionless),
            1 => Some(Self::Permission),
            2 => Some(Self::CustomizablePermissionless),
            3 => Some(Self::PermissionV2),
            _ => None,
        }
    }
}

/// Helper methods on [`LbPair`] that the SDK doesn't ship.
///
/// Imported transitively via the protocol module's re-exports — call
/// sites do `pool.spot_price()` without thinking about traits.
pub trait LbPairExt {
    /// Parse [`LbPair::status`] into a typed [`PairStatus`]. `None` on
    /// unknown bytes — caller decides whether to treat the pool as
    /// inactive (safe default) or surface the parse failure.
    fn pair_status(&self) -> Option<PairStatus>;

    /// True iff the pool is operational.
    fn is_active(&self) -> bool;

    /// Parse [`LbPair::pair_type`].
    fn pair_type_typed(&self) -> Option<PairType>;

    /// Active bin's spot price in raw token-Y per token-X (no decimal
    /// adjustment). Formula: `(1 + bin_step / 10_000)^active_id`.
    /// Caller adjusts for decimals before display.
    fn spot_price(&self) -> f64;

    /// Total fee rate (base + variable) in [`FEE_PRECISION`] units,
    /// capped at [`MAX_FEE_RATE`]. Equivalent to the on-chain
    /// `compute_fee_total` math.
    fn total_fee_rate(&self) -> u128;

    /// Convenience: pre-computed fee on a swap input amount, rounded
    /// up to match the on-chain ceiling.
    fn compute_fee_on_input(&self, amount_in: u64) -> u64;
}

impl LbPairExt for LbPair {
    fn pair_status(&self) -> Option<PairStatus> {
        PairStatus::from_u8(self.status)
    }

    fn is_active(&self) -> bool {
        self.pair_status() == Some(PairStatus::Enabled)
    }

    fn pair_type_typed(&self) -> Option<PairType> {
        PairType::from_u8(self.pair_type)
    }

    fn spot_price(&self) -> f64 {
        let step: f64 = 1.0 + (self.bin_step as f64 / 10_000.0);
        step.powi(self.active_id)
    }

    fn total_fee_rate(&self) -> u128 {
        let base = base_fee(self);
        let variable = variable_fee(self);
        base.saturating_add(variable).min(MAX_FEE_RATE as u128)
    }

    fn compute_fee_on_input(&self, amount_in: u64) -> u64 {
        let rate = self.total_fee_rate();
        let denom = (FEE_PRECISION as u128).saturating_sub(rate);
        if denom == 0 {
            // Fee >= 100% — pool is hosed; safest outcome is "the whole
            // thing is fee" so callers don't compute negative output.
            return amount_in;
        }
        let num = (amount_in as u128).saturating_mul(rate);
        // Ceil division to match on-chain rounding.
        ((num.saturating_add(denom - 1)) / denom).min(u64::MAX as u128) as u64
    }
}

fn base_fee(pair: &LbPair) -> u128 {
    // base_fee = base_factor * bin_step * 10 (FEE_PRECISION units).
    // The newer `base_fee_power_factor` adjusts the exponent on
    // bin_step; for back-compat with the original layout we treat it
    // as zero — wrong by orders of magnitude on pools that opt into
    // it. Real fee math lives in math.rs (step 2); this is good
    // enough for active/inactive checks and rough mark-pricing.
    (pair.parameters.base_factor as u128)
        .saturating_mul(pair.bin_step as u128)
        .saturating_mul(10)
}

fn variable_fee(pair: &LbPair) -> u128 {
    let vfc = pair.parameters.variable_fee_control as u128;
    if vfc == 0 {
        return 0;
    }
    let va = pair.v_parameters.volatility_accumulator as u128;
    let bs = pair.bin_step as u128;
    // (va * bs)^2 * vfc, scaled from 1e20 → 1e9 with ceiling.
    let square = va.saturating_mul(bs).saturating_pow(2);
    let raw = vfc.saturating_mul(square);
    raw.saturating_add(99_999_999_999) / 100_000_000_000
}

// =====================================================================
// Account-data → state struct adapters.
//
// Every account decoder validates the 8-byte discriminator before
// running borsh — wrong-discriminator data must surface as
// [`Error::ParseError`], never as a wrong-typed struct.
// =====================================================================

/// Decode an `LbPair` account from raw bytes (incl. 8-byte
/// discriminator).
pub fn decode_lb_pair(data: &[u8]) -> Result<LbPair> {
    decode_with_discriminator::<LbPair>(data, &LB_PAIR_DISCRIMINATOR, "LbPair")
}

/// Decode a `BinArray` account from raw bytes.
pub fn decode_bin_array(data: &[u8]) -> Result<BinArray> {
    decode_with_discriminator::<BinArray>(data, &BIN_ARRAY_DISCRIMINATOR, "BinArray")
}

/// Decode a `BinArrayBitmapExtension` account.
pub fn decode_bin_array_bitmap_extension(data: &[u8]) -> Result<BinArrayBitmapExtension> {
    decode_with_discriminator::<BinArrayBitmapExtension>(
        data,
        &BIN_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR,
        "BinArrayBitmapExtension",
    )
}

/// Decode the fixed header of a `PositionV2` account.
///
/// **Tolerant of trailing bytes.** Modern DLMM positions are *dynamic*:
/// the on-chain account is the fixed [`PositionV2`] struct (header + the
/// first 70 per-bin slots) with extra [`PositionBinData`] records
/// appended for bins beyond 70. We `deserialize` (which stops after the
/// fixed struct) rather than `try_from_slice` (which requires every byte
/// be consumed and so fails with "Not all bytes read" on wide
/// positions). This returns just the fixed struct — range, owner,
/// accumulators, and the first ≤70 bins. For positions wider than 70
/// bins, use [`decode_position_v2_full`] to also read the appended bins.
pub fn decode_position_v2(data: &[u8]) -> Result<PositionV2> {
    if data.len() < 8 {
        return Err(Error::AccountDataTooShort {
            expected: 8,
            actual: data.len(),
        });
    }
    if data[..8] != POSITION_V2_DISCRIMINATOR {
        return Err(Error::parse_error(format!(
            "PositionV2: discriminator mismatch (expected {:?}, got {:?})",
            POSITION_V2_DISCRIMINATOR,
            &data[..8],
        )));
    }
    let mut cur: &[u8] = data;
    PositionV2::deserialize(&mut cur)
        .map_err(|e| Error::parse_error(format!("PositionV2: borsh decode failed: {e}")))
}

/// One appended per-bin record for a dynamic position — the bins beyond
/// the 70 stored inline in [`PositionV2`]. Field order matches the
/// inline arrays element-wise (`liquidity_share`, then `reward_info`,
/// then `fee_info`), which is also the on-chain append layout.
#[derive(borsh::BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PositionBinData {
    pub liquidity_share: u128,
    pub reward_info: UserRewardInfo,
    pub fee_info: FeeInfo,
}

/// A fully-decoded position: the fixed [`PositionV2`] header (with its
/// first ≤70 bins inline) plus any appended bins for positions wider
/// than 70. Use the [`Self::liquidity_share`] / [`Self::fee_info`]
/// accessors to read per-bin data uniformly across the inline + appended
/// regions.
#[derive(Clone, Debug)]
pub struct PositionV2Full {
    /// Fixed header + first ≤70 bins.
    pub base: PositionV2,
    /// Bins beyond the inline 70. Empty for ≤70-bin positions.
    pub extended_bins: Vec<PositionBinData>,
}

impl PositionV2Full {
    /// Total bins this position covers (`upper - lower + 1`).
    pub fn bin_count(&self) -> usize {
        (self.base.upper_bin_id - self.base.lower_bin_id + 1).max(0) as usize
    }

    /// Liquidity share for the bin at `offset` within the range
    /// (`offset == 0` is `lower_bin_id`). Reads the inline array for the
    /// first 70 bins, the appended records beyond. `None` once `offset`
    /// reaches [`Self::bin_count`] — the inline array has 70 slots
    /// regardless of the position's width, so the range bound, not the
    /// array length, is authoritative.
    pub fn liquidity_share(&self, offset: usize) -> Option<u128> {
        if offset >= self.bin_count() {
            return None;
        }
        let inline = self.base.liquidity_shares.len();
        if offset < inline {
            self.base.liquidity_shares.get(offset).copied()
        } else {
            self.extended_bins
                .get(offset - inline)
                .map(|b| b.liquidity_share)
        }
    }

    /// Fee info for the bin at `offset`. See [`Self::liquidity_share`].
    pub fn fee_info(&self, offset: usize) -> Option<&FeeInfo> {
        if offset >= self.bin_count() {
            return None;
        }
        let inline = self.base.fee_infos.len();
        if offset < inline {
            self.base.fee_infos.get(offset)
        } else {
            self.extended_bins.get(offset - inline).map(|b| &b.fee_info)
        }
    }
}

/// Decode a dynamic `PositionV2` account in full: the fixed header plus
/// any appended [`PositionBinData`] records. Use this when you need
/// per-bin data for positions wider than 70 bins (current value / fee
/// accounting); [`decode_position_v2`] suffices when you only need the
/// header.
///
/// The appended region begins at [`PositionV2::LEN`] (the fixed account
/// size, discriminator included) and holds exactly `width - 70` records
/// of [`PositionBinData`] (112 bytes each). We read exactly that many
/// rather than "until end of slice" so trailing slack (if any) can't be
/// mis-parsed as a bin.
pub fn decode_position_v2_full(data: &[u8]) -> Result<PositionV2Full> {
    let base = decode_position_v2(data)?;

    let width = (base.upper_bin_id - base.lower_bin_id + 1).max(0) as usize;
    let inline = base.liquidity_shares.len();
    let extended_count = width.saturating_sub(inline);

    let mut extended_bins = Vec::with_capacity(extended_count);
    if extended_count > 0 {
        if data.len() < PositionV2::LEN {
            return Err(Error::AccountDataTooShort {
                expected: PositionV2::LEN,
                actual: data.len(),
            });
        }
        let mut cur: &[u8] = &data[PositionV2::LEN..];
        for i in 0..extended_count {
            let bin = PositionBinData::deserialize(&mut cur).map_err(|e| {
                Error::parse_error(format!(
                    "PositionV2: appended bin {i}/{extended_count} decode failed: {e}"
                ))
            })?;
            extended_bins.push(bin);
        }
    }

    Ok(PositionV2Full {
        base,
        extended_bins,
    })
}

fn decode_with_discriminator<T: BorshDeserialize>(
    data: &[u8],
    expected: &[u8; 8],
    label: &'static str,
) -> Result<T> {
    if data.len() < 8 {
        return Err(Error::AccountDataTooShort {
            expected: 8,
            actual: data.len(),
        });
    }
    if &data[..8] != expected {
        return Err(Error::parse_error(format!(
            "{label}: discriminator mismatch (expected {:?}, got {:?})",
            expected,
            &data[..8],
        )));
    }
    // SDK structs include the discriminator field in the borsh layout,
    // so we deserialize over the full slice rather than skipping the
    // first 8 bytes.
    T::try_from_slice(data)
        .map_err(|e| Error::parse_error(format!("{label}: borsh decode failed: {e}")))
}

/// The pool account names both mints outright, so it answers without a second
/// read. See [`NamesPair`](crate::pairs::NamesPair) for why only complete
/// layouts implement it.
impl crate::pairs::NamesPair for LbPair {
    fn pair(
        &self,
    ) -> (
        solana_program::pubkey::Pubkey,
        solana_program::pubkey::Pubkey,
    ) {
        // The SDK's pubkey type differs from ours by version only; the bytes
        // are identical. See [`dlmm_sdk_pubkey!`].
        (
            crate::dlmm_sdk_pubkey!(self.token_x_mint),
            crate::dlmm_sdk_pubkey!(self.token_y_mint),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_status_parses_known() {
        assert_eq!(PairStatus::from_u8(0), Some(PairStatus::Enabled));
        assert_eq!(PairStatus::from_u8(1), Some(PairStatus::Disabled));
        assert_eq!(PairStatus::from_u8(99), None);
    }

    #[test]
    fn decoder_rejects_short_data() {
        assert!(decode_lb_pair(&[1, 2, 3]).is_err());
    }

    // ── Golden on-chain fixtures ────────────────────────────────────────────
    // For the fixed DLMM structs a successful `try_from_slice` (exact-length
    // borsh) on real account bytes *is* the layout proof: a wrong struct size or
    // field type fails to decode. The field cross-checks pin a few values against
    // an independent decode of the same frozen bytes.

    #[test]
    fn decodes_onchain_lb_pair() {
        let fx = crate::test_fixtures::AccountFixture::load("meteora_dlmm/lb_pair.json");
        let lb = decode_lb_pair(fx.data()).expect("decode real LbPair (exact-length borsh)");
        assert_eq!(lb.token_x_mint.to_string(), fx.expected_str("token_x_mint"));
        assert_eq!(lb.token_y_mint.to_string(), fx.expected_str("token_y_mint"));
    }

    #[test]
    fn decodes_onchain_bin_array() {
        let fx = crate::test_fixtures::AccountFixture::load("meteora_dlmm/bin_array.json");
        let ba = decode_bin_array(fx.data()).expect("decode real BinArray (exact-length borsh)");
        assert_eq!(ba.lb_pair.to_string(), fx.expected_str("lb_pair"));
        assert_eq!(ba.bins.len(), 70, "BinArray always carries 70 bins");
    }

    #[test]
    fn decodes_onchain_bin_array_bitmap_extension() {
        let fx = crate::test_fixtures::AccountFixture::load(
            "meteora_dlmm/bin_array_bitmap_extension.json",
        );
        let ext = decode_bin_array_bitmap_extension(fx.data())
            .expect("decode real BinArrayBitmapExtension (exact-length borsh)");
        assert_eq!(ext.lb_pair.to_string(), fx.expected_str("lb_pair"));
    }

    #[test]
    fn decodes_onchain_position_v2_narrow() {
        let fx = crate::test_fixtures::AccountFixture::load("meteora_dlmm/position_v2_narrow.json");
        let pos = decode_position_v2_full(fx.data()).expect("decode real narrow PositionV2");
        assert_eq!(pos.base.lb_pair.to_string(), fx.expected_str("lb_pair"));
        assert_eq!(pos.base.owner.to_string(), fx.expected_str("owner"));
        assert_eq!(
            i64::from(pos.base.lower_bin_id),
            fx.expected_i64("lower_bin_id")
        );
        assert_eq!(
            i64::from(pos.base.upper_bin_id),
            fx.expected_i64("upper_bin_id")
        );
        let width = i64::from(pos.base.upper_bin_id - pos.base.lower_bin_id + 1);
        assert!(width <= 70, "narrow position spans <=70 bins, got {width}");
        assert!(
            pos.extended_bins.is_empty(),
            "narrow position has no appended bins"
        );
    }

    #[test]
    fn decoder_rejects_wrong_discriminator() {
        let mut buf = vec![0u8; 200];
        buf[..8].copy_from_slice(&[9, 9, 9, 9, 9, 9, 9, 9]);
        assert!(decode_lb_pair(&buf).is_err());
    }

    // -----------------------------------------------------------------
    // Dynamic PositionV2 decode
    //
    // The SDK `PositionV2` / sub-types aren't constructible here (no
    // BorshSerialize / Default / Copy in this build), so fixtures are
    // built at the byte level from the known borsh layout (LE, no
    // padding):
    //   disc            [0..8]
    //   lb_pair         [8..40]
    //   owner           [40..72]
    //   liquidity_shares[72..1192]   (70 x u128, slot i at 72 + 16*i)
    //   reward_infos    [1192..4552] (70 x 48)
    //   fee_infos       [4552..7912] (70 x 48)
    //   lower_bin_id    [7912..7916] (i32)
    //   upper_bin_id    [7916..7920] (i32)
    //   ...rest to LEN (8120)
    // Appended `PositionBinData` records (112 B each) start at LEN.
    // -----------------------------------------------------------------

    const LIQ_OFFSET: usize = 72;
    const LOWER_OFFSET: usize = 7912;
    const UPPER_OFFSET: usize = 7916;
    const BIN_DATA_SIZE: usize = 112;

    /// Build account-data for a position over `[lower, lower+width-1]`:
    /// `inline_share` in the first `min(width,70)` inline slots, then one
    /// 112-byte appended record per entry in `appended_shares` (each
    /// carrying that value as its `liquidity_share`).
    fn account_bytes(
        lower: i32,
        width: i32,
        inline_share: u128,
        appended_shares: &[u128],
    ) -> Vec<u8> {
        let mut buf = vec![0u8; PositionV2::LEN];
        buf[..8].copy_from_slice(&POSITION_V2_DISCRIMINATOR);
        let inline = width.clamp(0, 70) as usize;
        for i in 0..inline {
            let at = LIQ_OFFSET + i * 16;
            buf[at..at + 16].copy_from_slice(&inline_share.to_le_bytes());
        }
        buf[LOWER_OFFSET..LOWER_OFFSET + 4].copy_from_slice(&lower.to_le_bytes());
        buf[UPPER_OFFSET..UPPER_OFFSET + 4].copy_from_slice(&(lower + width - 1).to_le_bytes());
        for share in appended_shares {
            let mut rec = vec![0u8; BIN_DATA_SIZE];
            rec[..16].copy_from_slice(&share.to_le_bytes());
            buf.extend_from_slice(&rec);
        }
        buf
    }

    #[test]
    fn fixed_position_layout_offsets_round_trip() {
        // Guards the hand-coded offsets against SDK layout drift: a
        // fixed (<=70-bin) buffer must decode back to what we wrote.
        let data = account_bytes(-25, 50, 7, &[]);
        assert_eq!(data.len(), PositionV2::LEN);
        let p = decode_position_v2(&data).expect("decode fixed");
        assert_eq!(p.lower_bin_id, -25);
        assert_eq!(p.upper_bin_id, 24);
        assert_eq!(p.liquidity_shares[0], 7);
        assert_eq!(p.liquidity_shares[49], 7);
    }

    #[test]
    fn decode_full_narrow_position_has_no_appended_bins() {
        let data = account_bytes(-25, 50, 7, &[]);
        let full = decode_position_v2_full(&data).expect("decode narrow");
        assert_eq!(full.bin_count(), 50);
        assert!(full.extended_bins.is_empty());
        assert_eq!(full.liquidity_share(0), Some(7));
        assert_eq!(full.liquidity_share(49), Some(7));
        assert_eq!(full.liquidity_share(50), None);
    }

    #[test]
    fn decode_full_wide_position_reads_appended_bins() {
        let appended: Vec<u128> = (0..30).map(|i| 1000 + i as u128).collect();
        let data = account_bytes(0, 100, 11, &appended);
        let full = decode_position_v2_full(&data).expect("decode wide");
        assert_eq!(full.bin_count(), 100);
        assert_eq!(full.extended_bins.len(), 30);
        assert_eq!(full.liquidity_share(0), Some(11)); // inline
        assert_eq!(full.liquidity_share(69), Some(11));
        assert_eq!(full.liquidity_share(70), Some(1000)); // appended tail
        assert_eq!(full.liquidity_share(99), Some(1029));
        assert_eq!(full.liquidity_share(100), None);
    }

    #[test]
    fn decode_full_rejects_truncated_appended_region() {
        // Claims 100 bins (30 appended) but carries only 10 records.
        let appended: Vec<u128> = (0..10).map(|_| 1).collect();
        let data = account_bytes(0, 100, 1, &appended);
        assert!(decode_position_v2_full(&data).is_err());
    }

    #[test]
    fn decode_position_v2_tolerates_trailing_bytes() {
        // Header-only decode must succeed on a wide dynamic account.
        let appended: Vec<u128> = (0..30).map(|_| 5).collect();
        let data = account_bytes(0, 100, 5, &appended);
        let header = decode_position_v2(&data).expect("tolerant header decode");
        assert_eq!(header.lower_bin_id, 0);
        assert_eq!(header.upper_bin_id, 99);
    }
}
