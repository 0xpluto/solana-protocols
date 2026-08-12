//! Raydium CLMM account derivations and key management.
//!
//! Handles PDA derivation for tick arrays and observation state,
//! and provides [`RaydiumClmmKeys`] for instruction building.

use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;

use super::constants::{PROGRAM_ID, TICK_ARRAY_SIZE};
use super::state::PoolState;

/// All keys needed to build Raydium CLMM swap instructions.
///
/// Unlike Raydium V4 (which needs Serum accounts), CLMM pools are
/// self-contained — the pool state holds vault addresses, mints,
/// and tick spacing. The main complexity is tick array PDA derivation.
///
/// # Construction
///
/// Use [`RaydiumClmmKeys::from_pool_state`] to extract keys from a
/// parsed CLMM pool, or [`RaydiumClmmKeys::new`] when you know all
/// addresses.
#[derive(Debug, Clone)]
pub struct RaydiumClmmKeys {
    /// Pool state account address.
    pub pool_id: Pubkey,
    /// AMM config account (fee tiers, protocol fee ratios).
    pub amm_config: Pubkey,
    /// Token mint 0 (base).
    pub token_mint_0: Pubkey,
    /// Token mint 1 (quote).
    pub token_mint_1: Pubkey,
    /// Pool's token vault 0 (base).
    pub token_vault_0: Pubkey,
    /// Pool's token vault 1 (quote).
    pub token_vault_1: Pubkey,
    /// Observation state account.
    pub observation_key: Pubkey,
    /// Tick spacing (determines tick array granularity).
    pub tick_spacing: u16,
}

impl RaydiumClmmKeys {
    /// Create keys from individual fields.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool_id: Pubkey,
        amm_config: Pubkey,
        token_mint_0: Pubkey,
        token_mint_1: Pubkey,
        token_vault_0: Pubkey,
        token_vault_1: Pubkey,
        observation_key: Pubkey,
        tick_spacing: u16,
    ) -> Self {
        Self {
            pool_id,
            amm_config,
            token_mint_0,
            token_mint_1,
            token_vault_0,
            token_vault_1,
            observation_key,
            tick_spacing,
        }
    }

    /// Create keys from a parsed CLMM pool state.
    ///
    /// The pool address must be provided separately since it's not
    /// stored in the pool state data itself.
    #[must_use]
    pub fn from_pool_state(pool_address: Pubkey, pool: &PoolState) -> Self {
        Self {
            pool_id: pool_address,
            amm_config: pool.amm_config,
            token_mint_0: pool.token_mint_0,
            token_mint_1: pool.token_mint_1,
            token_vault_0: pool.token_vault_0,
            token_vault_1: pool.token_vault_1,
            observation_key: pool.observation_key,
            tick_spacing: pool.tick_spacing,
        }
    }

    /// Get the user's ATA for token 0 (base).
    #[must_use]
    pub fn user_token_0_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address(user, &self.token_mint_0)
    }

    /// Get the user's ATA for token 1 (quote).
    #[must_use]
    pub fn user_token_1_ata(&self, user: &Pubkey) -> Pubkey {
        get_associated_token_address(user, &self.token_mint_1)
    }

    /// Calculate the start tick index for the tick array containing `tick`.
    ///
    /// Each tick array holds `TICK_ARRAY_SIZE * tick_spacing` ticks in price space.
    /// The start index is always aligned to tick array boundaries.
    #[must_use]
    pub fn tick_array_start_for_tick(&self, tick: i32) -> i32 {
        tick_array_start_index(tick, self.tick_spacing)
    }

    /// Derive the tick array PDA for a given start tick index.
    #[must_use]
    pub fn tick_array_address(&self, start_tick_index: i32) -> Pubkey {
        derive_tick_array_address(&self.pool_id, start_tick_index)
    }

    /// Get tick array addresses needed for a swap in a given direction.
    ///
    /// Returns up to 3 tick arrays starting from the current tick,
    /// moving in the direction of the swap:
    /// - Buy (token0 → token1): price increases, tick moves up
    /// - Sell (token1 → token0): price decreases, tick moves down
    #[must_use]
    pub fn tick_arrays_for_swap(
        &self,
        current_tick: i32,
        is_buy: bool,
        count: usize,
    ) -> Vec<Pubkey> {
        let step = i32::from(self.tick_spacing) * TICK_ARRAY_SIZE;
        let start = tick_array_start_index(current_tick, self.tick_spacing);
        let mut addresses = Vec::with_capacity(count);

        for i in 0..count {
            let offset = if is_buy {
                start + step * i as i32
            } else {
                start - step * i as i32
            };
            addresses.push(derive_tick_array_address(&self.pool_id, offset));
        }

        addresses
    }
}

/// Calculate the start index for a tick array containing the given tick.
///
/// Formula: `floor(tick / (tick_spacing * TICK_ARRAY_SIZE)) * tick_spacing * TICK_ARRAY_SIZE`
#[must_use]
pub fn tick_array_start_index(tick: i32, tick_spacing: u16) -> i32 {
    let ticks_in_array = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    let mut start = tick / ticks_in_array * ticks_in_array;
    // Correct for negative ticks (floor division)
    if tick < 0 && tick % ticks_in_array != 0 {
        start -= ticks_in_array;
    }
    start
}

/// Derive the tick array PDA.
///
/// Seeds: `["tick_array", pool_id, start_tick_index_bytes]` under CLMM program.
#[must_use]
pub fn derive_tick_array_address(pool_id: &Pubkey, start_tick_index: i32) -> Pubkey {
    let (pda, _bump) = Pubkey::find_program_address(
        &[
            b"tick_array",
            pool_id.as_ref(),
            &start_tick_index.to_le_bytes(),
        ],
        &PROGRAM_ID,
    );
    pda
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_array_start_positive_tick() {
        // tick_spacing=1, TICK_ARRAY_SIZE=60 → ticks_in_array=60
        assert_eq!(tick_array_start_index(0, 1), 0);
        assert_eq!(tick_array_start_index(59, 1), 0);
        assert_eq!(tick_array_start_index(60, 1), 60);
        assert_eq!(tick_array_start_index(119, 1), 60);
        assert_eq!(tick_array_start_index(120, 1), 120);
    }

    #[test]
    fn tick_array_start_negative_tick() {
        // tick_spacing=1, ticks_in_array=60
        assert_eq!(tick_array_start_index(-1, 1), -60);
        assert_eq!(tick_array_start_index(-60, 1), -60);
        assert_eq!(tick_array_start_index(-61, 1), -120);
    }

    #[test]
    fn tick_array_start_with_spacing() {
        // tick_spacing=10, TICK_ARRAY_SIZE=60 → ticks_in_array=600
        assert_eq!(tick_array_start_index(0, 10), 0);
        assert_eq!(tick_array_start_index(599, 10), 0);
        assert_eq!(tick_array_start_index(600, 10), 600);
        assert_eq!(tick_array_start_index(-1, 10), -600);
        assert_eq!(tick_array_start_index(-600, 10), -600);
        assert_eq!(tick_array_start_index(-601, 10), -1200);
    }

    #[test]
    fn tick_array_pda_deterministic() {
        let pool = Pubkey::new_unique();
        let pda1 = derive_tick_array_address(&pool, 0);
        let pda2 = derive_tick_array_address(&pool, 0);
        assert_eq!(pda1, pda2);

        // Different start index → different PDA
        let pda3 = derive_tick_array_address(&pool, 60);
        assert_ne!(pda1, pda3);
    }

    #[test]
    fn keys_from_pool_state() {
        let pool_address = Pubkey::new_unique();
        let pool = PoolState {
            amm_config: Pubkey::new_unique(),
            token_mint_0: Pubkey::new_unique(),
            token_mint_1: Pubkey::new_unique(),
            token_vault_0: Pubkey::new_unique(),
            token_vault_1: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            tick_spacing: 10,
            ..PoolState::default()
        };

        let keys = RaydiumClmmKeys::from_pool_state(pool_address, &pool);

        assert_eq!(keys.pool_id, pool_address);
        assert_eq!(keys.amm_config, pool.amm_config);
        assert_eq!(keys.token_mint_0, pool.token_mint_0);
        assert_eq!(keys.token_mint_1, pool.token_mint_1);
        assert_eq!(keys.token_vault_0, pool.token_vault_0);
        assert_eq!(keys.token_vault_1, pool.token_vault_1);
        assert_eq!(keys.observation_key, pool.observation_key);
        assert_eq!(keys.tick_spacing, 10);
    }

    #[test]
    fn tick_arrays_for_swap_buy() {
        let keys = RaydiumClmmKeys {
            pool_id: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            token_mint_0: Pubkey::new_unique(),
            token_mint_1: Pubkey::new_unique(),
            token_vault_0: Pubkey::new_unique(),
            token_vault_1: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            tick_spacing: 1,
        };

        let arrays = keys.tick_arrays_for_swap(0, true, 3);
        assert_eq!(arrays.len(), 3);
        // Should be 3 unique addresses (consecutive tick arrays)
        assert_ne!(arrays[0], arrays[1]);
        assert_ne!(arrays[1], arrays[2]);
    }

    #[test]
    fn tick_arrays_for_swap_sell() {
        let keys = RaydiumClmmKeys {
            pool_id: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            token_mint_0: Pubkey::new_unique(),
            token_mint_1: Pubkey::new_unique(),
            token_vault_0: Pubkey::new_unique(),
            token_vault_1: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            tick_spacing: 1,
        };

        let arrays = keys.tick_arrays_for_swap(100, false, 3);
        assert_eq!(arrays.len(), 3);
        assert_ne!(arrays[0], arrays[1]);
        assert_ne!(arrays[1], arrays[2]);
    }

    #[test]
    fn user_atas() {
        let keys = RaydiumClmmKeys {
            pool_id: Pubkey::new_unique(),
            amm_config: Pubkey::new_unique(),
            token_mint_0: Pubkey::new_unique(),
            token_mint_1: Pubkey::new_unique(),
            token_vault_0: Pubkey::new_unique(),
            token_vault_1: Pubkey::new_unique(),
            observation_key: Pubkey::new_unique(),
            tick_spacing: 1,
        };
        let user = Pubkey::new_unique();

        assert_eq!(
            keys.user_token_0_ata(&user),
            get_associated_token_address(&user, &keys.token_mint_0)
        );
        assert_eq!(
            keys.user_token_1_ata(&user),
            get_associated_token_address(&user, &keys.token_mint_1)
        );
    }
}
