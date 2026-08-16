//! Derive macros for solana-protocols.
//!
//! This crate provides procedural macros to reduce boilerplate in protocol implementations:
//!
//! - [`InstructionData`] - Generate `to_data()` for instruction parameters
//! - [`AccountMetas`] - Generate `to_account_metas()` for account lists
//! - [`OnchainState`] - Generate `from_account_data()` from an account struct's fields
//! - [`LogParser`] - Generate log parsing for program events
//! - [`ProtocolInstruction`] - Generate instruction enum parsing
//!
//! # Discriminator Modes
//!
//! Both [`OnchainState`] and [`InstructionData`] support multiple discriminator modes:
//!
//! | Mode | Use Case |
//! |------|----------|
//! | 8-byte (default) | Anchor programs (Pumpfun, Raydium CLMM/CPMM) |
//! | 1-byte | Legacy programs with instruction indices (Raydium V4) |
//! | No discriminator | State accounts without discriminators |
//!
//! # Examples
//!
//! ## Standard Anchor (8-byte discriminator)
//!
//! ```ignore
//! #[derive(InstructionData)]
//! #[instruction_data(discriminator = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea])]
//! pub struct BuyParams {
//!     pub amount: u64,
//!     pub max_sol_cost: u64,
//! }
//!
//! #[derive(OnchainState)]
//! #[state(discriminator = anchor_account_discriminator!("BondingCurve"))]
//! pub struct BondingCurve { /* ... */ }
//! ```
//!
//! ## Legacy 1-byte Instruction Index (Raydium V4)
//!
//! ```ignore
//! #[derive(InstructionData)]
//! #[instruction_data(discriminator = [16], discriminator_size = 1)]
//! pub struct SwapBaseInParams {
//!     pub amount_in: u64,
//!     pub min_amount_out: u64,
//! }
//! ```
//!
//! ## No Discriminator (Raydium V4 State)
//!
//! ```ignore
//! #[derive(OnchainState)]
//! #[state(no_discriminator)]
//! pub struct RaydiumLiquidityV4 {
//!     pub status: u64,
//!     pub nonce: u64,
//!     // ... uses bincode directly, no discriminator
//! }
//! ```

use proc_macro::TokenStream;

mod account_metas;
mod build_accounts;
mod discriminator;
mod idl_check;
mod instruction_data;
mod log_parser;
mod onchain_account;
mod onchain_instruction;
mod onchain_state;
mod protocol_instruction;
mod quote_state;

/// Verify an instruction accounts-struct's parse side against a real landed
/// instruction: generates `impl VerifiedInstruction` + a round-trip
/// golden-fixture `#[test]`. Pairs with `#[derive(AccountMetas)]`. See
/// `onchain_instruction` for the contract.
#[proc_macro_derive(OnchainInstruction, attributes(onchain_ix))]
pub fn derive_onchain_instruction(input: TokenStream) -> TokenStream {
    onchain_instruction::derive(input)
}

/// Generate an accounts-struct's `derive(inputs…)` builder from declarative
/// per-field derivations (`input`/`key`/`pda`/`ata`) + an optional replay test
/// that rebuilds a real on-chain instruction from its own inputs. See
/// `build_accounts` for the contract.
#[proc_macro_derive(BuildAccounts, attributes(build))]
pub fn derive_build_accounts(input: TokenStream) -> TokenStream {
    build_accounts::derive(input)
}

/// Generate a verified `ProtocolStateHandler` from a handler struct: derived
/// discriminator, `deserialize` over a decode fn, an `impl VerifiedDecoder`, and
/// an emitted golden-fixture `#[test]`. See `onchain_account` for the full
/// contract. The author still writes the struct, `new()`, and `apply`.
#[proc_macro_derive(OnchainAccount, attributes(onchain))]
pub fn derive_onchain_account(input: TokenStream) -> TokenStream {
    onchain_account::derive(input)
}

/// Generate an on-chain account struct's parse from its field list, so the
/// fields *are* the byte layout and a field the account does not have cannot
/// exist. Version-added fields are trailing `Legacy<T>` marked
/// `#[state(added_in = "…")]`, where `Absent` means the account predates the
/// field — never a default, and deliberately not `Option<T>`, whose
/// combinators collapse that distinction. See `onchain_state` for the
/// full contract.
#[proc_macro_derive(OnchainState, attributes(state))]
pub fn derive_onchain_state(input: TokenStream) -> TokenStream {
    onchain_state::derive(input)
}

/// Generate a protocol's quote-state assembly **and** its dependency
/// declaration from one annotated struct, so the accounts a quote reads and the
/// accounts the ingest layer keeps live cannot drift apart. Per-field
/// `#[dep(root)]` / `#[dep(key = …, expect = …)]` / `#[dep(singleton)]` /
/// `#[dep(computed = …)]`; see `quote_state` for the full contract.
#[proc_macro_derive(QuoteState, attributes(dep))]
pub fn derive_quote_state(input: TokenStream) -> TokenStream {
    quote_state::derive(input)
}

/// Derive an Anchor **account** discriminator at compile time:
/// `anchor_account_discriminator!("Pool")` → `[u8; 8]` =
/// `sha256("account:Pool")[..8]`. No hand-typed bytes, no placeholder to leave
/// un-filled. Use as a `const` initializer or inside
/// `#[state(discriminator = anchor_account_discriminator!("Pool"))]`.
#[proc_macro]
pub fn anchor_account_discriminator(input: TokenStream) -> TokenStream {
    discriminator::account(input)
}

/// Derive an Anchor **instruction** discriminator at compile time:
/// `anchor_instruction_discriminator!("buy")` → `sha256("global:buy")[..8]`.
#[proc_macro]
pub fn anchor_instruction_discriminator(input: TokenStream) -> TokenStream {
    discriminator::instruction(input)
}

/// Derive an Anchor **event** discriminator at compile time:
/// `anchor_event_discriminator!("TradeEvent")` → `sha256("event:TradeEvent")[..8]`.
#[proc_macro]
pub fn anchor_event_discriminator(input: TokenStream) -> TokenStream {
    discriminator::event(input)
}

/// Derive macro for serializing instruction parameters to bytes.
///
/// Generates a `to_data(&self) -> Vec<u8>` method that serializes
/// the struct fields in order, prefixed with the discriminator.
///
/// # Attributes
///
/// - `#[instruction_data(discriminator = [0x00, ...])]` - Discriminator array
/// - `#[instruction_data(discriminator = CONST_NAME)]` - Reference to a const
/// - `#[instruction_data(discriminator_size = N)]` - Size in bytes (1, 2, 4, 8). Default: 8
/// - `#[instruction_data(no_discriminator)]` - Skip discriminator entirely
///
/// # Supported Field Types
///
/// - `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, `i128`
/// - `bool` (serialized as u8)
/// - `Pubkey` (32 bytes)
/// - `[u8; N]` (fixed-size arrays)
///
/// # Examples
///
/// ## 8-byte Anchor Discriminator (default)
///
/// ```ignore
/// #[derive(InstructionData)]
/// #[instruction_data(discriminator = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea])]
/// pub struct BuyParams {
///     pub amount: u64,
///     pub max_sol_cost: u64,
/// }
/// let data = params.to_data(); // 8 + 8 + 8 = 24 bytes
/// ```
///
/// ## 1-byte Instruction Index (Raydium V4)
///
/// ```ignore
/// #[derive(InstructionData)]
/// #[instruction_data(discriminator = [16], discriminator_size = 1)]
/// pub struct SwapBaseInParams {
///     pub amount_in: u64,
///     pub min_amount_out: u64,
/// }
/// let data = params.to_data(); // 1 + 8 + 8 = 17 bytes
/// ```
#[proc_macro_derive(InstructionData, attributes(instruction_data))]
pub fn derive_instruction_data(input: TokenStream) -> TokenStream {
    instruction_data::derive(input)
}

/// Derive macro for building AccountMeta vectors.
///
/// Generates a `to_account_metas(&self) -> Vec<AccountMeta>` method
/// that constructs the account list for an instruction.
///
/// # Field Attributes
///
/// - `#[account(writable)]` - Account is writable
/// - `#[account(signer)]` - Account is a signer
/// - `#[account(writable, signer)]` - Both
/// - No attribute = readonly, not signer
///
/// # Example
///
/// ```ignore
/// #[derive(AccountMetas)]
/// pub struct BuyAccounts {
///     #[account]
///     pub global: Pubkey,
///     #[account(writable)]
///     pub fee_recipient: Pubkey,
///     #[account(writable, signer)]
///     pub user: Pubkey,
/// }
///
/// let accounts = BuyAccounts { ... };
/// let metas = accounts.to_account_metas();
/// ```
#[proc_macro_derive(AccountMetas, attributes(account, idl))]
pub fn derive_account_metas(input: TokenStream) -> TokenStream {
    account_metas::derive(input)
}
/// Derive macro for parsing program logs into events.
///
/// Generates methods for parsing base64-encoded log data from
/// Solana program logs.
///
/// # Attributes
///
/// - `#[log_parser(discriminator = [0x00, 0x01, ...])]` - 8-byte event discriminator
/// - `#[log_parser(discriminator = CONST_NAME)]` - Reference to a const discriminator
/// - `#[log_parser(log_prefix = "Program log: ")]` - Optional log prefix to strip
///
/// # Example
///
/// ```ignore
/// #[derive(LogParser)]
/// #[log_parser(discriminator = [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d])]
/// pub struct TradeEvent {
///     pub mint: Pubkey,
///     pub sol_amount: u64,
///     pub token_amount: u64,
///     pub is_buy: bool,
///     pub user: Pubkey,
///     pub timestamp: i64,
/// }
///
/// // Parse from log line
/// if let Some(event) = TradeEvent::try_from_log(log_line) {
///     println!("Trade: {} tokens for {} SOL", event.token_amount, event.sol_amount);
/// }
/// ```
#[proc_macro_derive(LogParser, attributes(log_parser))]
pub fn derive_log_parser(input: TokenStream) -> TokenStream {
    log_parser::derive(input)
}

/// Derive macro for protocol instruction enums.
///
/// Generates comprehensive instruction parsing including:
/// - `try_from_slice(data)` - Parse bytes by discriminator matching
/// - `discriminator()` - Get discriminator for each variant
/// - `data()` - Serialize instruction back to bytes
/// - `from_accounts()` - Parse account pubkeys
/// - An Accounts enum matching instruction variants
/// - An Event struct combining instruction + accounts + log
///
/// # Enum Attributes
///
/// - `#[protocol(program_id = PROGRAM_ID)]` - Program ID constant
/// - `#[protocol(event_name = CustomEvent)]` - Custom event struct name
/// - `#[protocol(accounts_name = CustomAccounts)]` - Custom accounts enum name
///
/// # Variant Attributes
///
/// - `#[instruction(discriminator = DISC)]` - 8-byte discriminator (required)
/// - `#[instruction(accounts = AccountsStruct)]` - Accounts builder struct
/// - `#[instruction(log = LogStruct)]` - Log struct for this instruction
///
/// # Example
///
/// ```ignore
/// use solana_protocols_macros::ProtocolInstruction;
///
/// #[derive(ProtocolInstruction)]
/// #[protocol(program_id = PROGRAM_ID)]
/// pub enum PumpfunInstruction {
///     #[instruction(discriminator = BUY_DISC, accounts = BuyAccounts)]
///     Buy(BuyParams),
///
///     #[instruction(discriminator = SELL_DISC, accounts = SellAccounts)]
///     Sell(SellParams),
///
///     #[instruction(discriminator = CREATE_DISC, accounts = CreateAccounts)]
///     Create(CreateParams),
/// }
///
/// // Generated:
/// // - impl PumpfunInstruction { try_from_slice, discriminator, data, from_accounts }
/// // - enum PumpfunInstructionAccounts { Buy(BuyAccounts), Sell(SellAccounts), ... }
/// // - struct PumpfunInstructionEvent { instruction, accounts, log_data, logs_truncated }
/// ```
#[proc_macro_derive(ProtocolInstruction, attributes(protocol, instruction))]
pub fn derive_protocol_instruction(input: TokenStream) -> TokenStream {
    protocol_instruction::derive(input)
}
