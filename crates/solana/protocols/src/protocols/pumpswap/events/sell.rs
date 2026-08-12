//! PumpSwap `SellEvent` log parsing.
//!
//! Layout (borsh):
//! ```text
//! [  0..  8] timestamp                    i64
//! [  8.. 16] base_amount_in               u64
//! [ 16.. 24] min_quote_amount_out         u64
//! [ 24.. 32] user_base_token_reserves     u64
//! [ 32.. 40] user_quote_token_reserves    u64
//! [ 40.. 48] pool_base_token_reserves     u64
//! [ 48.. 56] pool_quote_token_reserves    u64
//! [ 56.. 64] quote_amount_out             u64
//! [ 64.. 72] lp_fee_basis_points          u64
//! [ 72.. 80] lp_fee                       u64
//! [ 80.. 88] protocol_fee_basis_points    u64
//! [ 88.. 96] protocol_fee                 u64
//! [ 96..104] quote_amount_out_without_lp_fee u64
//! [104..112] user_quote_amount_out        u64
//! [112..144] pool                         Pubkey (32)
//! [144..176] user                         Pubkey (32)
//! [176..208] user_base_token_account      Pubkey (32)
//! [208..240] user_quote_token_account     Pubkey (32)
//! [240..272] protocol_fee_recipient       Pubkey (32)
//! [272..304] protocol_fee_recipient_token_account Pubkey (32)
//! [304..336] coin_creator                 Pubkey (32) *
//! [336..344] coin_creator_fee_basis_points u64 *
//! [344..352] coin_creator_fee             u64 *
//! ```
//! Fields marked `*` are added in newer program versions.

use solana_program::pubkey::Pubkey;

/// Anchor `event:SellEvent` discriminator (`sha256("event:SellEvent")[..8]`).
pub const SELL_EVENT_DISCRIMINATOR: [u8; 8] =
    solana_protocols_macros::anchor_event_discriminator!("SellEvent");

/// Decoded `SellEvent` payload.
#[derive(Debug, Clone)]
pub struct SellEvent {
    pub timestamp: i64,
    pub base_amount_in: u64,
    pub min_quote_amount_out: u64,
    pub user_base_token_reserves: u64,
    pub user_quote_token_reserves: u64,
    pub pool_base_token_reserves: u64,
    pub pool_quote_token_reserves: u64,
    pub quote_amount_out: u64,
    pub lp_fee_basis_points: u64,
    pub lp_fee: u64,
    pub protocol_fee_basis_points: u64,
    pub protocol_fee: u64,
    pub quote_amount_out_without_lp_fee: u64,
    pub user_quote_amount_out: u64,
    pub pool: Pubkey,
    pub user: Pubkey,
    pub user_base_token_account: Pubkey,
    pub user_quote_token_account: Pubkey,
    pub protocol_fee_recipient: Pubkey,
    pub protocol_fee_recipient_token_account: Pubkey,
    pub coin_creator: Option<Pubkey>,
    pub coin_creator_fee_basis_points: Option<u64>,
    pub coin_creator_fee: Option<u64>,
}

impl SellEvent {
    pub const MIN_BODY_LEN: usize = 304;
    pub const EXTENDED_BODY_LEN: usize = 352;

    pub fn from_body(body: &[u8]) -> Option<Self> {
        if body.len() < Self::MIN_BODY_LEN {
            return None;
        }
        let timestamp = i64::from_le_bytes(body[0..8].try_into().ok()?);
        let base_amount_in = u64::from_le_bytes(body[8..16].try_into().ok()?);
        let min_quote_amount_out = u64::from_le_bytes(body[16..24].try_into().ok()?);
        let user_base_token_reserves = u64::from_le_bytes(body[24..32].try_into().ok()?);
        let user_quote_token_reserves = u64::from_le_bytes(body[32..40].try_into().ok()?);
        let pool_base_token_reserves = u64::from_le_bytes(body[40..48].try_into().ok()?);
        let pool_quote_token_reserves = u64::from_le_bytes(body[48..56].try_into().ok()?);
        let quote_amount_out = u64::from_le_bytes(body[56..64].try_into().ok()?);
        let lp_fee_basis_points = u64::from_le_bytes(body[64..72].try_into().ok()?);
        let lp_fee = u64::from_le_bytes(body[72..80].try_into().ok()?);
        let protocol_fee_basis_points = u64::from_le_bytes(body[80..88].try_into().ok()?);
        let protocol_fee = u64::from_le_bytes(body[88..96].try_into().ok()?);
        let quote_amount_out_without_lp_fee = u64::from_le_bytes(body[96..104].try_into().ok()?);
        let user_quote_amount_out = u64::from_le_bytes(body[104..112].try_into().ok()?);
        let pool = Pubkey::new_from_array(body[112..144].try_into().ok()?);
        let user = Pubkey::new_from_array(body[144..176].try_into().ok()?);
        let user_base_token_account = Pubkey::new_from_array(body[176..208].try_into().ok()?);
        let user_quote_token_account = Pubkey::new_from_array(body[208..240].try_into().ok()?);
        let protocol_fee_recipient = Pubkey::new_from_array(body[240..272].try_into().ok()?);
        let protocol_fee_recipient_token_account =
            Pubkey::new_from_array(body[272..304].try_into().ok()?);

        let (coin_creator, coin_creator_fee_basis_points, coin_creator_fee) =
            if body.len() >= Self::EXTENDED_BODY_LEN {
                let cc = Pubkey::new_from_array(body[304..336].try_into().ok()?);
                let bps = u64::from_le_bytes(body[336..344].try_into().ok()?);
                let fee = u64::from_le_bytes(body[344..352].try_into().ok()?);
                (Some(cc), Some(bps), Some(fee))
            } else {
                (None, None, None)
            };

        Some(SellEvent {
            timestamp,
            base_amount_in,
            min_quote_amount_out,
            user_base_token_reserves,
            user_quote_token_reserves,
            pool_base_token_reserves,
            pool_quote_token_reserves,
            quote_amount_out,
            lp_fee_basis_points,
            lp_fee,
            protocol_fee_basis_points,
            protocol_fee,
            quote_amount_out_without_lp_fee,
            user_quote_amount_out,
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
}
