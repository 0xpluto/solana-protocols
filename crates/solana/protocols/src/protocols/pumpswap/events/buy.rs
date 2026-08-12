//! PumpSwap `BuyEvent` log parsing.
//!
//! Layout (borsh, 273 bytes total):
//! ```text
//! [  0..  8] timestamp                    i64
//! [  8.. 16] base_amount_out              u64
//! [ 16.. 24] max_quote_amount_in          u64
//! [ 24.. 32] user_base_token_reserves     u64
//! [ 32.. 40] user_quote_token_reserves    u64
//! [ 40.. 48] pool_base_token_reserves     u64
//! [ 48.. 56] pool_quote_token_reserves    u64
//! [ 56.. 64] quote_amount_in              u64
//! [ 64.. 72] lp_fee_basis_points          u64
//! [ 72.. 80] lp_fee                       u64
//! [ 80.. 88] protocol_fee_basis_points    u64
//! [ 88.. 96] protocol_fee                 u64
//! [ 96..104] quote_amount_in_with_lp_fee  u64
//! [104..112] user_quote_amount_in         u64
//! [112..144] pool                         Pubkey (32)
//! [144..176] user                         Pubkey (32)
//! [176..208] user_base_token_account      Pubkey (32)
//! [208..240] user_quote_token_account     Pubkey (32)
//! [240..272] protocol_fee_recipient       Pubkey (32) *
//! [272..304] protocol_fee_recipient_token_account  Pubkey (32) *
//! [304..336] coin_creator                 Pubkey (32) *
//! [336..344] coin_creator_fee_basis_points u64 *
//! [344..352] coin_creator_fee             u64 *
//! [352..353] track_volume                 bool *
//! [353..361] total_unclaimed_tokens       u64 *
//! [361..369] total_claimed_tokens         u64 *
//! [369..377] current_sol_volume           u64 *
//! [377..385] last_update_timestamp        i64 *
//! ```
//! Fields marked `*` are added in newer program versions; older
//! events truncate at byte 240.

use solana_program::pubkey::Pubkey;

/// Anchor `event:BuyEvent` discriminator (`sha256("event:BuyEvent")[..8]`).
pub const BUY_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("BuyEvent");

/// Decoded `BuyEvent` payload.
///
/// Mirrors the on-chain Anchor struct field-for-field. All
/// `Option<...>` fields are populated only when the on-chain event
/// log carries them; older program versions truncate at
/// `protocol_fee_recipient`.
#[derive(Debug, Clone)]
pub struct BuyEvent {
    pub timestamp: i64,
    pub base_amount_out: u64,
    pub max_quote_amount_in: u64,
    pub user_base_token_reserves: u64,
    pub user_quote_token_reserves: u64,
    pub pool_base_token_reserves: u64,
    pub pool_quote_token_reserves: u64,
    pub quote_amount_in: u64,
    pub lp_fee_basis_points: u64,
    pub lp_fee: u64,
    pub protocol_fee_basis_points: u64,
    pub protocol_fee: u64,
    pub quote_amount_in_with_lp_fee: u64,
    pub user_quote_amount_in: u64,
    pub pool: Pubkey,
    pub user: Pubkey,
    pub user_base_token_account: Pubkey,
    pub user_quote_token_account: Pubkey,
    pub protocol_fee_recipient: Pubkey,
    pub protocol_fee_recipient_token_account: Pubkey,
    /// Creator of the underlying token (only set on post-graduation
    /// pumpfun mints; pure PumpSwap pools may have a default value).
    pub coin_creator: Option<Pubkey>,
    pub coin_creator_fee_basis_points: Option<u64>,
    pub coin_creator_fee: Option<u64>,
}

impl BuyEvent {
    /// Smallest BuyEvent body size: through `protocol_fee_recipient_token_account`.
    pub const MIN_BODY_LEN: usize = 304;
    /// Larger body that includes coin_creator + creator-fee fields.
    pub const EXTENDED_BODY_LEN: usize = 352;

    /// Parse the borsh-serialized body (no discriminator prefix).
    /// Returns `None` if the input is shorter than `MIN_BODY_LEN`.
    /// Trailing fields beyond `MIN_BODY_LEN` are read opportunistically.
    pub fn from_body(body: &[u8]) -> Option<Self> {
        if body.len() < Self::MIN_BODY_LEN {
            return None;
        }
        let timestamp = i64::from_le_bytes(body[0..8].try_into().ok()?);
        let base_amount_out = u64::from_le_bytes(body[8..16].try_into().ok()?);
        let max_quote_amount_in = u64::from_le_bytes(body[16..24].try_into().ok()?);
        let user_base_token_reserves = u64::from_le_bytes(body[24..32].try_into().ok()?);
        let user_quote_token_reserves = u64::from_le_bytes(body[32..40].try_into().ok()?);
        let pool_base_token_reserves = u64::from_le_bytes(body[40..48].try_into().ok()?);
        let pool_quote_token_reserves = u64::from_le_bytes(body[48..56].try_into().ok()?);
        let quote_amount_in = u64::from_le_bytes(body[56..64].try_into().ok()?);
        let lp_fee_basis_points = u64::from_le_bytes(body[64..72].try_into().ok()?);
        let lp_fee = u64::from_le_bytes(body[72..80].try_into().ok()?);
        let protocol_fee_basis_points = u64::from_le_bytes(body[80..88].try_into().ok()?);
        let protocol_fee = u64::from_le_bytes(body[88..96].try_into().ok()?);
        let quote_amount_in_with_lp_fee = u64::from_le_bytes(body[96..104].try_into().ok()?);
        let user_quote_amount_in = u64::from_le_bytes(body[104..112].try_into().ok()?);
        let pool = Pubkey::new_from_array(body[112..144].try_into().ok()?);
        let user = Pubkey::new_from_array(body[144..176].try_into().ok()?);
        let user_base_token_account = Pubkey::new_from_array(body[176..208].try_into().ok()?);
        let user_quote_token_account = Pubkey::new_from_array(body[208..240].try_into().ok()?);
        let protocol_fee_recipient = Pubkey::new_from_array(body[240..272].try_into().ok()?);
        let protocol_fee_recipient_token_account =
            Pubkey::new_from_array(body[272..304].try_into().ok()?);

        // Optional extension fields.
        let (coin_creator, coin_creator_fee_basis_points, coin_creator_fee) =
            if body.len() >= Self::EXTENDED_BODY_LEN {
                let cc = Pubkey::new_from_array(body[304..336].try_into().ok()?);
                let bps = u64::from_le_bytes(body[336..344].try_into().ok()?);
                let fee = u64::from_le_bytes(body[344..352].try_into().ok()?);
                (Some(cc), Some(bps), Some(fee))
            } else {
                (None, None, None)
            };

        Some(BuyEvent {
            timestamp,
            base_amount_out,
            max_quote_amount_in,
            user_base_token_reserves,
            user_quote_token_reserves,
            pool_base_token_reserves,
            pool_quote_token_reserves,
            quote_amount_in,
            lp_fee_basis_points,
            lp_fee,
            protocol_fee_basis_points,
            protocol_fee,
            quote_amount_in_with_lp_fee,
            user_quote_amount_in,
            pool,
            user,
            user_base_token_account,
            user_quote_token_account,
            protocol_fee_recipient,
            protocol_fee_recipient_token_account,
            coin_creator,
            coin_creator_fee_basis_points,
            coin_creator_fee,
        })
    }

    /// Total user-paid quote (SOL) including all fees that left the pool.
    /// `quote_amount_in_with_lp_fee` is what entered pool reserves;
    /// `protocol_fee` and `coin_creator_fee` were taken before the
    /// pool deposit. Sum is the gross amount the user spent.
    pub fn gross_quote_in(&self) -> u64 {
        self.quote_amount_in_with_lp_fee + self.protocol_fee + self.coin_creator_fee.unwrap_or(0)
    }
}
