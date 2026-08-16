//! `Shareholder` — a creator-fee split entry.
//!
//! Declared by name in **both** pump IDLs (`pump.json` and `pump_amm.json`),
//! which is why it sits here beside [`OptionBool`](super::OptionBool) rather
//! than inside either protocol: it is a pump-family standard, and a type the
//! venue-neutral [`CreatorFee`](crate::chain::CreatorFee) has to be able to
//! name without importing one protocol into the shared vocabulary.

use solana_program::pubkey::Pubkey;

/// A creator-fee shareholder, as the IDL declares it.
///
/// Shares are basis points, deliberately left unresolved to amounts: the
/// program does that rounding, and doing it here would put our arithmetic
/// behind the chain's authority.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    borsh::BorshDeserialize,
    borsh::BorshSerialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Shareholder {
    /// Wallet receiving a share.
    pub address: Pubkey,
    /// Share in basis points.
    pub share_bps: u16,
}
