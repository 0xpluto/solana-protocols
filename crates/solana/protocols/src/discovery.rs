//! Pool discovery and protocol detection.
//!
//! This module provides functions to identify protocols from on-chain data.
//! It's designed to support multi-protocol routing and pool discovery.
//!
//! # Adding a New Protocol
//!
//! When adding a new protocol, update:
//! 1. [`detect_protocol_from_data`] - Add discriminator check
//! 2. [`detect_protocol_from_program`] - Add program ID mapping
//!
//! The functions use exhaustive matching internally to ensure all protocols
//! are handled.

use solana_program::pubkey::Pubkey;

use crate::protocols::{pumpfun, pumpswap, raydium_v4, Protocol};

/// Account discriminator (first 8 bytes) for protocol detection.
pub type Discriminator = [u8; 8];

/// Try to detect protocol from account data discriminator.
///
/// Checks the first 8 bytes of account data against known protocol
/// discriminators. This is useful when you have the account data but
/// don't know which protocol it belongs to.
///
/// # Returns
///
/// `Some(Protocol)` if the discriminator matches a known protocol,
/// `None` otherwise.
///
/// # Example
///
/// ```ignore
/// let data = client.get_account_data(&pool_address)?;
/// if let Some(protocol) = detect_protocol_from_data(&data) {
///     let state = PoolState::from_account_data(protocol, &data)?;
/// }
/// ```
#[must_use]
pub fn detect_protocol_from_data(data: &[u8]) -> Option<Protocol> {
    if data.len() < 8 {
        return None;
    }

    let discriminator: [u8; 8] = data[..8].try_into().ok()?;

    // Check each protocol's discriminator
    // Order by expected frequency for performance
    if discriminator == pumpfun::BONDING_CURVE_DISCRIMINATOR {
        return Some(Protocol::Pumpfun);
    }

    // Future protocols would be checked here:
    // if discriminator == raydium_v4::POOL_DISCRIMINATOR {
    //     return Some(Protocol::RaydiumV4);
    // }
    // if discriminator == raydium_clmm::POOL_DISCRIMINATOR {
    //     return Some(Protocol::RaydiumClmm);
    // }

    None
}

/// Try to detect protocol from program ID.
///
/// This is a simple wrapper around [`Protocol::from_program_id`] that
/// provides a symmetric API with [`detect_protocol_from_data`].
///
/// # Returns
///
/// `Some(Protocol)` if the program ID matches a known protocol,
/// `None` otherwise.
#[must_use]
pub fn detect_protocol_from_program(program_id: &Pubkey) -> Option<Protocol> {
    Protocol::from_program_id(program_id)
}

/// Get the expected discriminator for a protocol's pool state.
///
/// Useful for pre-filtering accounts before attempting full parsing.
/// Note: Some protocols (like Raydium V4) have no discriminator - returns zeros.
///
/// # Adding a New Protocol
///
/// Add a match arm returning the protocol's pool state discriminator.
#[must_use]
pub fn protocol_discriminator(protocol: Protocol) -> Discriminator {
    match protocol {
        Protocol::Pumpfun => pumpfun::BONDING_CURVE_DISCRIMINATOR,
        Protocol::PumpSwap => pumpswap::POOL_DISCRIMINATOR,
        // Raydium V4 has no discriminator (legacy non-Anchor program)
        Protocol::RaydiumV4 => [0u8; 8],
        // Discriminators for these protocols will be added when state parsing is implemented
        Protocol::RaydiumClmm
        | Protocol::RaydiumCpmm
        | Protocol::RaydiumLaunchpad
        | Protocol::MeteoraDlmm
        | Protocol::MeteoraDbC
        | Protocol::MeteoraDammV2 => [0u8; 8],
    }
}

/// Get the expected account size for a protocol's pool state.
///
/// Useful for pre-filtering accounts by size before parsing.
///
/// # Adding a New Protocol
///
/// Add a match arm returning the protocol's pool state account size.
#[must_use]
pub fn protocol_account_size(protocol: Protocol) -> usize {
    match protocol {
        Protocol::Pumpfun => pumpfun::BONDING_CURVE_ACCOUNT_SIZE,
        Protocol::PumpSwap => pumpswap::POOL_ACCOUNT_SIZE,
        Protocol::RaydiumV4 => raydium_v4::POOL_STATE_SIZE,
        // Account sizes for these protocols will be added when state parsing is implemented.
        // Return 0 for now — callers should check protocol support before using this.
        Protocol::RaydiumClmm
        | Protocol::RaydiumCpmm
        | Protocol::RaydiumLaunchpad
        | Protocol::MeteoraDlmm
        | Protocol::MeteoraDbC
        | Protocol::MeteoraDammV2 => 0,
    }
}

/// Check if account data could belong to a specific protocol.
///
/// Performs quick checks (discriminator and size) without full parsing.
/// Useful for filtering before expensive RPC calls.
#[must_use]
pub fn is_protocol_account(protocol: Protocol, data: &[u8]) -> bool {
    let expected_size = protocol_account_size(protocol);
    if data.len() < expected_size {
        return false;
    }

    let expected_discriminator = protocol_discriminator(protocol);
    if data.len() < 8 {
        return false;
    }

    data[..8] == expected_discriminator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_pumpfun_from_data() {
        let mut data = vec![0u8; pumpfun::BONDING_CURVE_ACCOUNT_SIZE];
        data[..8].copy_from_slice(&pumpfun::BONDING_CURVE_DISCRIMINATOR);

        let protocol = detect_protocol_from_data(&data);
        assert_eq!(protocol, Some(Protocol::Pumpfun));
    }

    #[test]
    fn detect_unknown_data() {
        let data = vec![0xFF; 100];
        let protocol = detect_protocol_from_data(&data);
        assert!(protocol.is_none());
    }

    #[test]
    fn detect_short_data() {
        let data = vec![0u8; 4];
        let protocol = detect_protocol_from_data(&data);
        assert!(protocol.is_none());
    }

    #[test]
    fn detect_from_program_id() {
        let protocol = detect_protocol_from_program(&pumpfun::PROGRAM_ID);
        assert_eq!(protocol, Some(Protocol::Pumpfun));

        let unknown = detect_protocol_from_program(&Pubkey::new_unique());
        assert!(unknown.is_none());
    }

    #[test]
    fn protocol_discriminators() {
        let disc = protocol_discriminator(Protocol::Pumpfun);
        assert_eq!(disc, pumpfun::BONDING_CURVE_DISCRIMINATOR);
    }

    #[test]
    fn is_protocol_account_valid() {
        let mut data = vec![0u8; pumpfun::BONDING_CURVE_ACCOUNT_SIZE];
        data[..8].copy_from_slice(&pumpfun::BONDING_CURVE_DISCRIMINATOR);

        assert!(is_protocol_account(Protocol::Pumpfun, &data));
    }

    #[test]
    fn is_protocol_account_wrong_discriminator() {
        let mut data = vec![0u8; pumpfun::BONDING_CURVE_ACCOUNT_SIZE];
        data[..8].copy_from_slice(&[0xFF; 8]);

        assert!(!is_protocol_account(Protocol::Pumpfun, &data));
    }

    #[test]
    fn is_protocol_account_too_short() {
        let data = vec![0u8; 10];
        assert!(!is_protocol_account(Protocol::Pumpfun, &data));
    }
}
