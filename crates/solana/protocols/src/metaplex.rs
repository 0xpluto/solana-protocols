//! Metaplex Token Metadata — the classic-SPL half of "what is this mint
//! called". Read-only: PDA derivation plus the account decode for the three
//! display fields the token dimension consumes (name / symbol / uri).
//!
//! Deliberately not the `mpl-token-metadata` crate: we read three fields from
//! a stable prefix layout, and the crate drags an IDL-generated surface this
//! repo has no other use for. The layout is pinned by a golden mainnet
//! fixture (`fixtures/metaplex/`), per the decoder-verification doctrine —
//! a hand-written layout is admissible only with chain truth beside it.

use solana_program::pubkey::Pubkey;

/// The MPL Token Metadata program.
pub const MPL_TOKEN_METADATA: Pubkey =
    solana_program::pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

/// Derive the metadata PDA for a mint: `["metadata", program, mint]`.
#[must_use]
pub fn metadata_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"metadata", MPL_TOKEN_METADATA.as_ref(), mint.as_ref()],
        &MPL_TOKEN_METADATA,
    )
    .0
}

/// The display fields of a mint, wherever they were read from (Metaplex PDA
/// or the Token-2022 metadata extension).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintDisplay {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

/// Errors decoding a metadata account.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("account too short: {0} bytes at field {1}")]
    Truncated(usize, &'static str),
    #[error("not a metadata account (key byte {0})")]
    WrongKind(u8),
    #[error("field {0} is not UTF-8")]
    NotUtf8(&'static str),
}

/// Decode the display fields from a Metaplex metadata account.
///
/// Layout (stable since MetadataV1): `key: u8` (4 = MetadataV1),
/// `update_authority: [u8;32]`, `mint: [u8;32]`, then three borsh strings
/// (u32 LE length + bytes) whose *contents* are zero-padded to fixed
/// capacity (name 32, symbol 10, uri 200) — the padding is inside the
/// string, so trim trailing NULs after decoding.
pub fn decode_metadata(data: &[u8]) -> Result<MintDisplay, MetadataError> {
    const METADATA_V1: u8 = 4;
    let key = *data.first().ok_or(MetadataError::Truncated(0, "key"))?;
    if key != METADATA_V1 {
        return Err(MetadataError::WrongKind(key));
    }
    let mut off = 1 + 32 + 32;
    let mut string = |field: &'static str| -> Result<String, MetadataError> {
        let len_end = off + 4;
        let raw_len = data
            .get(off..len_end)
            .ok_or(MetadataError::Truncated(data.len(), field))?;
        let len = u32::from_le_bytes(raw_len.try_into().expect("4 bytes")) as usize;
        let bytes = data
            .get(len_end..len_end + len)
            .ok_or(MetadataError::Truncated(data.len(), field))?;
        off = len_end + len;
        Ok(std::str::from_utf8(bytes)
            .map_err(|_| MetadataError::NotUtf8(field))?
            .trim_end_matches('\0')
            .to_string())
    };
    Ok(MintDisplay {
        name: string("name")?,
        symbol: string("symbol")?,
        uri: string("uri")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn pda_derivation_prints_and_is_stable() {
        // HxQh… is the classic-SPL fixture mint; the derived PDA must match
        // the address the fixture was captured from.
        let mint: Pubkey = "HxQhDGYqyjorgogMJx7YbBHADEDxuHhLnMMmr6VYpyn"
            .parse()
            .unwrap();
        println!("metadata_pda = {}", metadata_pda(&mint));
    }

    #[test]
    fn decodes_the_golden_mainnet_fixture() {
        let raw = include_str!("../fixtures/metaplex/metadata_hxqh.json");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let mint: Pubkey = v["mint"].as_str().unwrap().parse().unwrap();
        let addr: Pubkey = v["address"].as_str().unwrap().parse().unwrap();
        assert_eq!(metadata_pda(&mint), addr, "PDA derivation matches capture");
        let data = base64::engine::general_purpose::STANDARD
            .decode(v["account_b64"].as_str().unwrap())
            .unwrap();
        let m = decode_metadata(&data).unwrap();
        assert_eq!(m.name, v["expected_name"].as_str().unwrap());
        assert_eq!(m.symbol, v["expected_symbol"].as_str().unwrap());
        assert!(!m.uri.is_empty());
    }

    #[test]
    fn refuses_non_metadata_bytes() {
        assert!(matches!(
            decode_metadata(&[9u8; 80]),
            Err(MetadataError::WrongKind(9))
        ));
        assert!(decode_metadata(&[]).is_err());
    }
}
