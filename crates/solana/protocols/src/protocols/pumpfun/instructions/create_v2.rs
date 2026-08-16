//! Pump.fun `create_v2` instruction types.
//!
//! Pumpfun moved most live token launches to `create_v2` in 2024.
//! Both the on-chain account list and the borsh args differ from v1
//! enough that they're modelled as separate structs rather than
//! unified.
//!
//! # Wire layout (from the on-chain IDL)
//!
//! Accounts (16 total):
//!
//! ```text
//! [ 0] mint                    [writable, signer]
//! [ 1] mint_authority
//! [ 2] bonding_curve           [writable]
//! [ 3] associated_bonding_curve [writable]
//! [ 4] global
//! [ 5] user                    [writable, signer]   ← the creator wallet
//! [ 6] system_program
//! [ 7] token_program                                 ← v1's `user` slot
//! [ 8] associated_token_program
//! [ 9] mayhem_program_id       [writable]
//! \[10\] global_params
//! \[11\] sol_vault               [writable]
//! \[12\] mayhem_state            [writable]
//! \[13\] mayhem_token_vault      [writable]
//! \[14\] event_authority
//! \[15\] program
//! ```
//!
//! Args (after the 8-byte discriminator):
//!
//! ```text
//! name:                string  (4-byte LE length + UTF-8)
//! symbol:              string
//! uri:                 string
//! creator:             Pubkey (32 bytes)
//! is_mayhem_mode:      bool   (1 byte)
//! is_cashback_enabled: OptionBool — see TrackVolume; the IDL defines it as a
//!                      struct wrapping a bool (ONE byte), and senders also emit
//!                      the absent and two-byte forms.
//! ```

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use solana_protocols_macros::AccountMetas;

use crate::parsing::{FromInstructionData, InstructionParseError};

use super::super::constants::CREATE_V2_DISCRIMINATOR;

/// Account list for the `create_v2` instruction. 16 slots.
#[derive(Debug, Clone, AccountMetas)]
pub struct CreateV2Accounts {
    /// Token mint (created by this instruction).
    #[account(writable, signer)]
    pub mint: Pubkey,
    /// Mint authority PDA.
    #[account]
    pub mint_authority: Pubkey,
    /// Bonding curve PDA — the pricing primitive for this mint.
    #[account(writable)]
    pub bonding_curve: Pubkey,
    /// Bonding curve's associated token account.
    #[account(writable)]
    pub associated_bonding_curve: Pubkey,
    /// Global state.
    #[account]
    pub global: Pubkey,
    /// Creator wallet — signs the tx and pays. **This is the
    /// "creator" address downstream consumers should index on.**
    #[account(writable, signer)]
    pub user: Pubkey,
    /// System program.
    #[account]
    pub system_program: Pubkey,
    /// SPL Token program.
    #[account]
    pub token_program: Pubkey,
    /// Associated token program.
    #[account]
    pub associated_token_program: Pubkey,
    /// Mayhem program.
    #[account(writable)]
    pub mayhem_program_id: Pubkey,
    /// Global params.
    #[account]
    pub global_params: Pubkey,
    /// SOL vault for mayhem mode / cashback escrow.
    #[account(writable)]
    pub sol_vault: Pubkey,
    /// Mayhem state account.
    #[account(writable)]
    pub mayhem_state: Pubkey,
    /// Mayhem token vault.
    #[account(writable)]
    pub mayhem_token_vault: Pubkey,
    /// Anchor event authority PDA.
    #[account]
    pub event_authority: Pubkey,
    /// Pumpfun program ID (Anchor self-referential CPI guard).
    #[account]
    pub program: Pubkey,
}

/// Parameters for the `create_v2` instruction. Strict superset of
/// [`CreateParams`](super::CreateParams).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateV2Params {
    /// Token name.
    pub name: String,
    /// Token symbol.
    pub symbol: String,
    /// Metadata URI.
    pub uri: String,
    /// Explicit creator wallet recorded on-chain. May differ from the
    /// signer at [`CreateV2Accounts::user`] when a launch service or
    /// proxy signs on behalf of an end-user — though in practice the
    /// two usually match.
    pub creator: Pubkey,
    /// Whether the bonding curve launches in mayhem mode (changes
    /// fee recipient + reserved-recipient routing — see
    /// `PumpfunGlobal::reserved_fee_recipients`).
    pub is_mayhem_mode: bool,
    /// Cashback flag as it appeared on the wire. See [`TrackVolume`] for why
    /// this is not an `Option<bool>`: the encoding is inconsistent on chain and
    /// the form itself is evidence about the sender.
    ///
    /// [`TrackVolume`]: crate::protocols::pumpfun::TrackVolume
    pub is_cashback_enabled: super::super::TrackVolume,
}

impl CreateV2Params {
    /// Convenience: this instruction's discriminator. Useful for tests
    /// that hand-craft instruction data.
    pub const DISCRIMINATOR: [u8; 8] = CREATE_V2_DISCRIMINATOR;
}

impl FromInstructionData for CreateV2Params {
    fn from_instruction_data(data: &[u8]) -> Result<Self, InstructionParseError> {
        let mut offset = 0;

        let name = read_borsh_string(data, &mut offset)?;
        let symbol = read_borsh_string(data, &mut offset)?;
        let uri = read_borsh_string(data, &mut offset)?;
        let creator = read_pubkey(data, &mut offset)?;
        let is_mayhem_mode = read_bool(data, &mut offset)?;
        // `OptionBool` is not borsh's `Option<bool>`. The IDL defines it as a
        // struct wrapping a bool — one byte — and senders additionally emit the
        // absent and two-byte forms. Reading it as a tagged option consumed a
        // value byte that is not there, which is why every live 132-byte
        // create_v2 failed to parse while the discriminator dispatched fine.
        let is_cashback_enabled = super::super::TrackVolume::from_bytes(&data[offset..])
            .map_err(|e| InstructionParseError::DeserializationFailed(e.to_string()))?;

        Ok(Self {
            name,
            symbol,
            uri,
            creator,
            is_mayhem_mode,
            is_cashback_enabled,
        })
    }
}

// ---------------------------------------------------------------------
// Borsh primitive helpers (local — keep create_v2.rs self-contained)
// ---------------------------------------------------------------------

fn read_borsh_string(data: &[u8], offset: &mut usize) -> Result<String, InstructionParseError> {
    if *offset + 4 > data.len() {
        return Err(InstructionParseError::DeserializationFailed(format!(
            "create_v2: not enough data for string length at offset {offset}"
        )));
    }
    let len = u32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap()) as usize;
    *offset += 4;
    if *offset + len > data.len() {
        return Err(InstructionParseError::DeserializationFailed(format!(
            "create_v2: string length {len} exceeds data at offset {offset}"
        )));
    }
    let s = std::str::from_utf8(&data[*offset..*offset + len])
        .map_err(|e| {
            InstructionParseError::DeserializationFailed(format!("create_v2: invalid UTF-8: {e}"))
        })?
        .to_string();
    *offset += len;
    Ok(s)
}

fn read_pubkey(data: &[u8], offset: &mut usize) -> Result<Pubkey, InstructionParseError> {
    if *offset + 32 > data.len() {
        return Err(InstructionParseError::DeserializationFailed(format!(
            "create_v2: not enough data for pubkey at offset {offset}"
        )));
    }
    let bytes: [u8; 32] = data[*offset..*offset + 32].try_into().unwrap();
    *offset += 32;
    Ok(Pubkey::from(bytes))
}

fn read_bool(data: &[u8], offset: &mut usize) -> Result<bool, InstructionParseError> {
    if *offset + 1 > data.len() {
        return Err(InstructionParseError::DeserializationFailed(format!(
            "create_v2: not enough data for bool at offset {offset}"
        )));
    }
    let v = data[*offset] != 0;
    *offset += 1;
    Ok(v)
}

/// Anchor `OptionBool` is encoded with a single tag byte (`0` =
/// `None`, `1` = `Some`) followed by the value byte when `Some`.

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    fn encode_string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    #[test]
    fn parses_full_v2_payload() {
        let mut data = Vec::new();
        encode_string(&mut data, "Test V2");
        encode_string(&mut data, "TV2");
        encode_string(&mut data, "ipfs://abc");
        data.extend_from_slice(pk(0xAA).as_ref()); // creator
        data.push(1); // is_mayhem_mode = true
        data.push(1); // OptionBool tag = Some
        data.push(0); // OptionBool value = false

        let p = CreateV2Params::from_instruction_data(&data).expect("parse");
        assert_eq!(p.name, "Test V2");
        assert_eq!(p.symbol, "TV2");
        assert_eq!(p.uri, "ipfs://abc");
        assert_eq!(p.creator, pk(0xAA));
        assert!(p.is_mayhem_mode);
        // `[1, 0]` is the two-byte form. Under borsh Option it reads
        // Some(false); as the canonical single byte plus a trailing byte it
        // reads true-then-junk. We record the bytes and decline to resolve it.
        assert_eq!(
            p.is_cashback_enabled,
            crate::protocols::pumpfun::TrackVolume::SomeFalseExtra
        );
    }

    #[test]
    fn parses_v2_with_canonical_single_byte_optionbool() {
        let mut data = Vec::new();
        encode_string(&mut data, "X");
        encode_string(&mut data, "X");
        encode_string(&mut data, "");
        data.extend_from_slice(pk(0xBB).as_ref());
        data.push(0); // is_mayhem_mode = false
        data.push(0); // canonical OptionBool: a single byte = false

        let p = CreateV2Params::from_instruction_data(&data).expect("parse");
        assert!(!p.is_mayhem_mode);
        // This asserted `None` before 2026-08-12, reading `[0]` as borsh's
        // Option tag. The IDL defines OptionBool as a struct wrapping a bool,
        // so a lone `[0]` is canonical FALSE, not absent — and that misreading
        // is why every live create_v2 failed to parse.
        assert_eq!(
            p.is_cashback_enabled,
            crate::protocols::pumpfun::TrackVolume::SomeFalse
        );
    }

    #[test]
    fn rejects_truncated_v2() {
        // Three strings + creator pubkey, but then truncated mid-bool.
        let mut data = Vec::new();
        encode_string(&mut data, "X");
        encode_string(&mut data, "X");
        encode_string(&mut data, "X");
        data.extend_from_slice(pk(0xCC).as_ref());
        // missing is_mayhem_mode byte
        let err = CreateV2Params::from_instruction_data(&data).unwrap_err();
        assert!(matches!(
            err,
            InstructionParseError::DeserializationFailed(_)
        ));
    }
}
