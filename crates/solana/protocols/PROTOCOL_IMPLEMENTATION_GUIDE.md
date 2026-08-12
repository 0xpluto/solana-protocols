# Protocol Implementation Guide

This guide explains how to add a new protocol to `solana-protocols`. The crate uses a **compiler-driven** approach where adding a protocol variant causes compiler errors at every location needing protocol-specific logic.

## Design Principles

1. **Exhaustive Matching**: No wildcard `_` patterns. Adding a new variant forces handling everywhere.
2. **Minimal Boilerplate**: Use derive macros (`StateParser`, `AccountMetas`, etc.) to reduce repetitive code.
3. **Unified Abstractions**: All protocols produce `SwapOutput` for consistent handling.
4. **Type Safety**: Protocol-specific types wrapped in unified enums (`PoolState`, `PoolKeys`).

## Quick Reference

```
Step 1: Add Protocol variant
Step 2: Create protocol module structure
Step 3: Fix all compiler errors (they guide you)
Step 4: Add tests
```

---

## Step 1: Add Protocol Variant

### 1.1 Add to Protocol enum

**File:** `src/protocols/mod.rs`

```rust
pub enum Protocol {
    Pumpfun,
    RaydiumV4,  // <- ADD YOUR VARIANT
}
```

### 1.2 Add to PoolState enum

**File:** `src/protocols/mod.rs`

```rust
pub enum PoolState {
    Pumpfun(pumpfun::BondingCurve),
    RaydiumV4(raydium_v4::PoolState),  // <- ADD YOUR VARIANT
}
```

### 1.3 Add to PoolKeys enum

**File:** `src/protocols/mod.rs`

```rust
pub enum PoolKeys {
    Pumpfun(pumpfun::PumpfunKeys),
    RaydiumV4(raydium_v4::PoolKeys),  // <- ADD YOUR VARIANT
}
```

### 1.4 Run `cargo build`

The compiler will now show every location needing updates. Fix them one by one.

---

## Step 2: Create Protocol Module

### 2.1 Directory Structure

Create your protocol module following this exact structure:

```
src/protocols/your_protocol/
├── mod.rs           # Module root, re-exports
├── constants.rs     # Program IDs, discriminators, fees
├── accounts.rs      # PDA derivations, pool keys struct
├── state/
│   ├── mod.rs       # Re-exports state types
│   └── pool.rs      # Pool state struct with StateParser
├── instructions/
│   ├── mod.rs       # Re-exports instruction builders
│   ├── swap.rs      # Swap instruction builder
│   └── common.rs    # Shared instruction helpers
├── math.rs          # SwapMath implementation
└── events/          # (optional) Log parsing
    └── mod.rs
```

### 2.2 constants.rs

Define program IDs, discriminators, and fee constants:

```rust
//! Protocol constants.

use solana_program::pubkey::Pubkey;

/// Program ID (get from protocol docs or on-chain).
pub const PROGRAM_ID: Pubkey = solana_program::pubkey!("YourProgramId...");

/// Pool state account discriminator (first 8 bytes).
/// Usually the hash of the account struct name.
pub const POOL_DISCRIMINATOR: [u8; 8] = [0x00, 0x01, ...];

/// Pool state account size in bytes.
pub const POOL_ACCOUNT_SIZE: usize = 8 + /* your struct size */;

/// Fee constants
pub const FEE_BPS: u16 = 30; // 0.30%
pub const FEE_DENOMINATOR: u64 = 10_000;
```

### 2.3 state/pool.rs

Use the `StateParser` derive macro:

```rust
//! Pool state.

use solana_program::pubkey::Pubkey;
use solana_protocols_macros::StateParser;
use serde::{Deserialize, Serialize};

use super::super::constants::POOL_DISCRIMINATOR;

/// Pool state from on-chain account.
#[derive(Debug, Clone, Serialize, Deserialize, StateParser)]
#[state_parser(discriminator = POOL_DISCRIMINATOR)]
pub struct PoolState {
    /// Reserve A.
    pub reserve_a: u64,
    /// Reserve B.
    pub reserve_b: u64,
    // ... other fields matching on-chain layout
}

impl PoolState {
    /// Check if pool is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        // Protocol-specific logic
        true
    }

    /// Get spot price.
    #[must_use]
    pub fn spot_price(&self) -> f64 {
        // Protocol-specific price calculation
        self.reserve_a as f64 / self.reserve_b as f64
    }
}
```

### 2.4 accounts.rs

Define PDA derivations and pool keys:

```rust
//! Account derivations.

use solana_program::pubkey::Pubkey;

use super::constants::PROGRAM_ID;

/// All keys needed for swap instructions.
#[derive(Debug, Clone)]
pub struct PoolKeys {
    pub pool: Pubkey,
    pub vault_a: Pubkey,
    pub vault_b: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
}

impl PoolKeys {
    /// Create from known addresses.
    #[must_use]
    pub fn new(pool: Pubkey, mint_a: Pubkey, mint_b: Pubkey) -> Self {
        // Derive vault PDAs
        let vault_a = derive_vault_pda(&pool, &mint_a);
        let vault_b = derive_vault_pda(&pool, &mint_b);

        Self {
            pool,
            vault_a,
            vault_b,
            mint_a,
            mint_b,
        }
    }
}

/// Derive vault PDA.
#[must_use]
pub fn derive_vault_pda(pool: &Pubkey, mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[b"vault", pool.as_ref(), mint.as_ref()],
        &PROGRAM_ID,
    );
    pda
}
```

### 2.5 math.rs

Implement `SwapMath` trait:

```rust
//! Swap math.

use crate::error::{Error, Result};
use crate::events::{SwapOutput, SwapOutputBuilder};
use crate::traits::{SwapAmount, SwapDirection, SwapMath, SwapParams};

use super::constants::FEE_BPS;
use super::state::PoolState;

impl SwapMath for PoolState {
    fn calculate_swap(&self, params: &SwapParams) -> Result<SwapOutput> {
        match (params.direction, &params.amount) {
            (SwapDirection::Buy, SwapAmount::ExactIn(amount)) => {
                self.calculate_buy(*amount)
            }
            (SwapDirection::Buy, SwapAmount::ExactOut(amount)) => {
                self.calculate_buy_exact_out(*amount)
            }
            (SwapDirection::Sell, SwapAmount::ExactIn(amount)) => {
                self.calculate_sell(*amount)
            }
            (SwapDirection::Sell, SwapAmount::ExactOut(amount)) => {
                self.calculate_sell_exact_out(*amount)
            }
        }
    }

    fn spot_price(&self) -> f64 {
        PoolState::spot_price(self)
    }

    fn is_active(&self) -> bool {
        PoolState::is_active(self)
    }
}

impl PoolState {
    /// Calculate buy: input A → output B.
    pub fn calculate_buy(&self, amount_in: u64) -> Result<SwapOutput> {
        // Your swap math here
        // Use constant product, concentrated liquidity, etc.

        Ok(SwapOutputBuilder::new()
            .amount_in(amount_in)
            .amount_out(calculated_output)
            .protocol_fee(fee)
            .build())
    }

    // Implement other methods...
}
```

### 2.6 instructions/swap.rs

Use `AccountMetas` derive macro:

```rust
//! Swap instruction builder.

use solana_program::pubkey::Pubkey;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_protocols_macros::AccountMetas;

use crate::error::Result;
use crate::traits::InstructionBuilder;

use super::super::constants::PROGRAM_ID;
use super::super::accounts::PoolKeys;

/// Swap instruction discriminator.
const SWAP_DISCRIMINATOR: [u8; 8] = [0x00, 0x01, ...];

/// Swap parameters.
#[derive(Debug, Clone)]
pub struct SwapParams {
    pub amount_in: u64,
    pub min_amount_out: u64,
}

/// Swap instruction accounts.
#[derive(AccountMetas)]
pub struct SwapAccounts {
    pub pool: Pubkey,
    #[account(writable)]
    pub vault_a: Pubkey,
    #[account(writable)]
    pub vault_b: Pubkey,
    #[account(writable, signer)]
    pub user: Pubkey,
    #[account(writable)]
    pub user_token_a: Pubkey,
    #[account(writable)]
    pub user_token_b: Pubkey,
    pub token_program: Pubkey,
}

/// Swap instruction builder.
pub struct SwapBuilder;

impl InstructionBuilder for SwapBuilder {
    type Keys = PoolKeys;
    type Params = SwapParams;

    fn build_swap_instruction(
        keys: &Self::Keys,
        user: &Pubkey,
        params: Self::Params,
    ) -> Result<Instruction> {
        let accounts = SwapAccounts {
            pool: keys.pool,
            vault_a: keys.vault_a,
            vault_b: keys.vault_b,
            user: *user,
            user_token_a: derive_user_ata(user, &keys.mint_a),
            user_token_b: derive_user_ata(user, &keys.mint_b),
            token_program: spl_token::id(),
        };

        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&SWAP_DISCRIMINATOR);
        data.extend_from_slice(&params.amount_in.to_le_bytes());
        data.extend_from_slice(&params.min_amount_out.to_le_bytes());

        Ok(Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts.to_account_metas(),
            data,
        })
    }
}
```

---

## Step 3: Fix Compiler Errors

After adding your protocol module, run `cargo build`. You'll see errors like:

```
error[E0004]: non-exhaustive patterns: `RaydiumV4` not covered
  --> src/protocols/mod.rs:123:15
   |
   | match protocol {
   |       ^^^^^^^^ pattern `RaydiumV4` not covered
```

Fix each one by adding the appropriate match arm:

```rust
// In Protocol::program_id()
match self {
    Protocol::Pumpfun => pumpfun::PROGRAM_ID,
    Protocol::RaydiumV4 => raydium_v4::PROGRAM_ID,  // ADD
}

// In PoolState::from_account_data()
match protocol {
    Protocol::Pumpfun => { /* existing */ }
    Protocol::RaydiumV4 => {                        // ADD
        let pool = raydium_v4::PoolState::from_account_data(data)?;
        Ok(PoolState::RaydiumV4(pool))
    }
}
```

### Files That Need Updates

The compiler will guide you, but typically:

1. **`src/protocols/mod.rs`**
   - `Protocol::program_id()`
   - `Protocol::name()`
   - `Protocol::short_name()`
   - `Protocol::is_bonding_curve()`
   - `Protocol::is_concentrated_liquidity()`
   - `Protocol::base_token()`
   - `Protocol::all()`
   - `Protocol::from_program_id()`
   - `PoolState::from_account_data()`
   - `PoolState::detect_and_parse()`
   - `PoolState::protocol()`
   - `PoolState::is_active()`
   - `PoolState::spot_price()`
   - `SwapMath for PoolState`
   - `PoolKeys::protocol()`
   - `PoolKeys::mint()`
   - `PoolKeys::pool_address()`

2. **`src/discovery.rs`**
   - `detect_protocol_from_data()`
   - `protocol_discriminator()`
   - `protocol_account_size()`

---

## Step 4: Add Tests

### 4.1 Unit Tests

Add tests in each module file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> PoolState {
        PoolState {
            reserve_a: 1_000_000_000,
            reserve_b: 1_000_000_000,
            // ...
        }
    }

    #[test]
    fn calculate_swap() {
        let pool = test_pool();
        let params = SwapParams::buy(1_000_000);
        let output = pool.calculate_swap(&params).unwrap();

        assert!(output.amount_out > 0);
        assert!(output.fee > 0);
    }
}
```

### 4.2 Integration Tests

Add protocol tests in `src/protocols/mod.rs`:

```rust
#[test]
fn your_protocol_pool_state() {
    let pool = your_protocol::PoolState { /* ... */ };
    let data = make_account_data(&pool);

    let state = PoolState::from_account_data(Protocol::YourProtocol, &data).unwrap();
    assert_eq!(state.protocol(), Protocol::YourProtocol);
}
```

---

## Checklist

- [ ] Added `Protocol::YourProtocol` variant
- [ ] Added `PoolState::YourProtocol(...)` variant
- [ ] Added `PoolKeys::YourProtocol(...)` variant
- [ ] Created `protocols/your_protocol/` module structure
- [ ] Defined constants (PROGRAM_ID, discriminator, account size)
- [ ] Implemented pool state with `#[derive(StateParser)]`
- [ ] Implemented `SwapMath` for pool state
- [ ] Implemented pool keys with PDA derivations
- [ ] Implemented instruction builder with `#[derive(AccountMetas)]`
- [ ] Fixed all compiler errors
- [ ] Added unit tests for math calculations
- [ ] Added integration tests for PoolState/PoolKeys
- [ ] Updated discovery functions
- [ ] Ran `cargo test` - all pass
- [ ] Ran `cargo clippy` - no warnings

---

## Common Patterns

### Slippage

Use the `Slippage` struct:

```rust
use crate::traits::Slippage;

let slippage = Slippage::new(100); // 1% (100 bps)
let min_out = slippage.apply_min(expected_out);
let max_in = slippage.apply_max(expected_in);

// Or from percentage
let slippage = Slippage::from_percent(0.5); // 0.5%
```

### Swap Output

Always use `SwapOutputBuilder`:

```rust
SwapOutputBuilder::new()
    .amount_in(sol_in)
    .amount_out(tokens_out)
    .protocol_fee(protocol_fee)
    .lp_fee(lp_fee)
    .price_impact_bps(impact)
    .effective_price(price)
    .build()
```

### Token Programs

Support both SPL Token and Token 2022:

```rust
use crate::tokens::{TokenProgram, TokenWithProgram};

// Auto-detect from ATA
let token = TokenWithProgram::detect(&ata, &owner, &mint)?;

// Build instruction for correct program
let ix = token.create_ata_instruction(&owner, &payer);
```

---

## Reference Implementation

See `protocols/pumpfun/` for a complete reference implementation of a bonding curve protocol.
