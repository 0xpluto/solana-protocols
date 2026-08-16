//! Protocol registry for O(1) instruction dispatch.
//!
//! The [`ProtocolRegistry`] provides efficient protocol lookup by program ID
//! using enum dispatch (no vtable overhead).
//!
//! # Event Types
//!
//! The parser returns [`ProtocolEvent`] which can represent different actions:
//! - [`SwapEvent`] - Token swaps (buy/sell)
//! - [`TokenCreationEvent`] - New token/pool creation
//!
//! # Example
//!
//! ```ignore
//! let registry = ProtocolRegistry::with_all_protocols();
//!
//! // Parse instructions
//! for ix in instructions {
//!     match registry.parse(&ix)? {
//!         Some(ProtocolEvent::Swap(swap)) => println!("Swap: {:?}", swap),
//!         Some(ProtocolEvent::TokenCreation(create)) => println!("New token: {}", create.name),
//!         None => {} // Unknown instruction type
//!     }
//! }
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use solana_program::pubkey::Pubkey;
use thiserror::Error;

use super::instruction::ParsedInstruction;
use crate::protocols::Protocol;

/// Error from instruction parsing.
#[derive(Debug, Clone, Error)]
pub enum InstructionParseError {
    /// No parser registered for this program ID.
    #[error("unknown program: {0}")]
    UnknownProgram(Pubkey),

    /// Instruction discriminator not recognized (8-byte Anchor style).
    #[error("unknown instruction discriminator: {0:?}")]
    UnknownDiscriminator([u8; 8]),

    /// Instruction discriminator not recognized (variable length).
    #[error("unknown instruction discriminator: {0:?}")]
    UnknownDiscriminatorVec(Vec<u8>),

    /// Instruction data too short for discriminator.
    #[error("instruction data too short for discriminator")]
    DataTooShort,

    /// Instruction data too short with details.
    #[error("instruction data too short: expected {expected}, got {actual}")]
    DataTooShortDetailed { expected: usize, actual: usize },

    /// Failed to deserialize instruction data.
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),

    /// Required log not found.
    #[error("required log not found: {0}")]
    LogNotFound(String),

    /// Required account missing.
    #[error("missing account at index {0}")]
    MissingAccount(usize),

    /// Not enough accounts provided.
    #[error("not enough accounts: expected {expected}, got {actual}")]
    NotEnoughAccounts { expected: usize, actual: usize },

    /// Invalid account data.
    #[error("invalid account at index {index}: {reason}")]
    InvalidAccount { index: usize, reason: String },

    /// Invalid field value.
    #[error("invalid field: {0}")]
    InvalidField(String),
}

// =============================================================================
// Protocol Events
// =============================================================================

/// Unified event from any protocol instruction.
///
/// This enum wraps all possible event types that can be parsed from
/// protocol instructions. Use pattern matching to handle each type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProtocolEvent {
    /// A token swap (buy or sell).
    Swap(SwapEvent),

    /// A new token/pool creation.
    TokenCreation(TokenCreationEvent),

    /// A new AMM pool created.
    PoolCreation(PoolCreationEvent),

    /// Liquidity added to a pool.
    LiquidityAdd(LiquidityAddEvent),

    /// Liquidity removed from a pool.
    LiquidityRemove(LiquidityRemoveEvent),
}

impl ProtocolEvent {
    /// Get the protocol that produced this event.
    pub fn protocol(&self) -> Protocol {
        match self {
            ProtocolEvent::Swap(e) => e.protocol,
            ProtocolEvent::TokenCreation(e) => e.protocol,
            ProtocolEvent::PoolCreation(e) => e.protocol,
            ProtocolEvent::LiquidityAdd(e) => e.protocol,
            ProtocolEvent::LiquidityRemove(e) => e.protocol,
        }
    }

    /// Check if this is a swap event.
    pub fn is_swap(&self) -> bool {
        matches!(self, ProtocolEvent::Swap(_))
    }

    /// Check if this is a token creation event.
    pub fn is_token_creation(&self) -> bool {
        matches!(self, ProtocolEvent::TokenCreation(_))
    }

    /// Check if this is a pool creation event.
    pub fn is_pool_creation(&self) -> bool {
        matches!(self, ProtocolEvent::PoolCreation(_))
    }

    /// Check if this is a liquidity add event.
    pub fn is_liquidity_add(&self) -> bool {
        matches!(self, ProtocolEvent::LiquidityAdd(_))
    }

    /// Check if this is a liquidity remove event.
    pub fn is_liquidity_remove(&self) -> bool {
        matches!(self, ProtocolEvent::LiquidityRemove(_))
    }

    /// Try to get as swap event.
    pub fn as_swap(&self) -> Option<&SwapEvent> {
        match self {
            ProtocolEvent::Swap(e) => Some(e),
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => None,
        }
    }

    /// Try to get as token creation event.
    pub fn as_token_creation(&self) -> Option<&TokenCreationEvent> {
        match self {
            ProtocolEvent::TokenCreation(e) => Some(e),
            ProtocolEvent::Swap(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => None,
        }
    }

    /// Try to get as pool creation event.
    pub fn as_pool_creation(&self) -> Option<&PoolCreationEvent> {
        match self {
            ProtocolEvent::PoolCreation(e) => Some(e),
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => None,
        }
    }

    /// Try to get as liquidity add event.
    pub fn as_liquidity_add(&self) -> Option<&LiquidityAddEvent> {
        match self {
            ProtocolEvent::LiquidityAdd(e) => Some(e),
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityRemove(_) => None,
        }
    }

    /// Try to get as liquidity remove event.
    pub fn as_liquidity_remove(&self) -> Option<&LiquidityRemoveEvent> {
        match self {
            ProtocolEvent::LiquidityRemove(e) => Some(e),
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_) => None,
        }
    }
}

/// Unified swap event from any protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapEvent {
    /// Protocol that executed the swap.
    pub protocol: Protocol,

    /// Pool/curve address.
    pub pool: Pubkey,

    /// Input token mint.
    pub token_in: Pubkey,

    /// Output token mint.
    pub token_out: Pubkey,

    /// Input amount (smallest units).
    pub amount_in: u64,

    /// Output amount (smallest units).
    pub amount_out: u64,

    /// Fee paid (smallest units of fee token).
    pub fee: Option<u64>,

    /// User/trader address.
    pub user: Option<Pubkey>,

    /// Whether this was a buy (base → quote) or sell (quote → base).
    pub is_buy: Option<bool>,

    /// Protocol-specific data (JSON for extensibility).
    pub extra: Option<serde_json::Value>,
}

impl SwapEvent {
    /// Create a new swap event.
    #[must_use]
    pub fn new(
        protocol: Protocol,
        pool: Pubkey,
        token_in: Pubkey,
        token_out: Pubkey,
        amount_in: u64,
        amount_out: u64,
    ) -> Self {
        Self {
            protocol,
            pool,
            token_in,
            token_out,
            amount_in,
            amount_out,
            fee: None,
            user: None,
            is_buy: None,
            extra: None,
        }
    }

    /// Set the fee.
    #[must_use]
    pub fn with_fee(mut self, fee: u64) -> Self {
        self.fee = Some(fee);
        self
    }

    /// Set the user.
    #[must_use]
    pub fn with_user(mut self, user: Pubkey) -> Self {
        self.user = Some(user);
        self
    }

    /// Set buy/sell direction.
    #[must_use]
    pub fn with_direction(mut self, is_buy: bool) -> Self {
        self.is_buy = Some(is_buy);
        self
    }

    /// Set extra protocol-specific data.
    #[must_use]
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }

    /// Calculate effective price (output per input).
    #[must_use]
    pub fn effective_price(&self, in_decimals: u8, out_decimals: u8) -> f64 {
        if self.amount_in == 0 {
            return 0.0;
        }
        let in_ui = self.amount_in as f64 / 10f64.powi(in_decimals as i32);
        let out_ui = self.amount_out as f64 / 10f64.powi(out_decimals as i32);
        out_ui / in_ui
    }
}

/// Token creation event (new token/pool launched).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCreationEvent {
    /// Protocol where token was created.
    pub protocol: Protocol,

    /// Token mint address.
    pub mint: Pubkey,

    /// Bonding curve / pool address.
    pub pool: Pubkey,

    /// Token name.
    pub name: String,

    /// Token symbol.
    pub symbol: String,

    /// Metadata URI (IPFS or HTTP).
    pub uri: String,

    /// Creator address.
    pub creator: Option<Pubkey>,

    /// Protocol-specific data.
    pub extra: Option<serde_json::Value>,
}

impl TokenCreationEvent {
    /// Create a new token creation event.
    #[must_use]
    pub fn new(
        protocol: Protocol,
        mint: Pubkey,
        pool: Pubkey,
        name: String,
        symbol: String,
        uri: String,
    ) -> Self {
        Self {
            protocol,
            mint,
            pool,
            name,
            symbol,
            uri,
            creator: None,
            extra: None,
        }
    }

    /// Set the creator.
    #[must_use]
    pub fn with_creator(mut self, creator: Pubkey) -> Self {
        self.creator = Some(creator);
        self
    }

    /// Set extra data.
    #[must_use]
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

/// Pool creation event (new AMM pool created).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolCreationEvent {
    /// Protocol where pool was created.
    pub protocol: Protocol,

    /// Pool account address.
    pub pool: Pubkey,

    /// First token mint (base/token A).
    pub token_a: Pubkey,

    /// Second token mint (quote/token B).
    pub token_b: Pubkey,

    /// Initial amount of token A deposited.
    pub initial_amount_a: u64,

    /// Initial amount of token B deposited.
    pub initial_amount_b: u64,

    /// Pool creator address.
    pub creator: Option<Pubkey>,

    /// LP token mint (if applicable).
    pub lp_mint: Option<Pubkey>,

    /// Protocol-specific data.
    pub extra: Option<serde_json::Value>,
}

impl PoolCreationEvent {
    /// Create a new pool creation event.
    #[must_use]
    pub fn new(
        protocol: Protocol,
        pool: Pubkey,
        token_a: Pubkey,
        token_b: Pubkey,
        initial_amount_a: u64,
        initial_amount_b: u64,
    ) -> Self {
        Self {
            protocol,
            pool,
            token_a,
            token_b,
            initial_amount_a,
            initial_amount_b,
            creator: None,
            lp_mint: None,
            extra: None,
        }
    }

    /// Set the creator.
    #[must_use]
    pub fn with_creator(mut self, creator: Pubkey) -> Self {
        self.creator = Some(creator);
        self
    }

    /// Set the LP mint.
    #[must_use]
    pub fn with_lp_mint(mut self, lp_mint: Pubkey) -> Self {
        self.lp_mint = Some(lp_mint);
        self
    }

    /// Set extra data.
    #[must_use]
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

/// Liquidity added to a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityAddEvent {
    /// Protocol.
    pub protocol: Protocol,

    /// Pool account address.
    pub pool: Pubkey,

    /// User who added liquidity.
    pub user: Pubkey,

    /// First token mint.
    pub token_a: Pubkey,

    /// Second token mint.
    pub token_b: Pubkey,

    /// Amount of token A deposited.
    pub amount_a: u64,

    /// Amount of token B deposited.
    pub amount_b: u64,

    /// LP tokens minted (constant product AMMs).
    pub lp_amount: Option<u64>,

    /// Liquidity units added (CLMM/DLMM concentrated liquidity).
    pub liquidity_delta: Option<u128>,

    /// Lower tick bound (CLMM only).
    pub tick_lower: Option<i32>,

    /// Upper tick bound (CLMM only).
    pub tick_upper: Option<i32>,

    /// Lower bin ID (DLMM only).
    pub bin_id_lower: Option<i32>,

    /// Upper bin ID (DLMM only).
    pub bin_id_upper: Option<i32>,

    /// Protocol-specific data.
    pub extra: Option<serde_json::Value>,
}

impl LiquidityAddEvent {
    /// Create a new liquidity add event.
    #[must_use]
    pub fn new(
        protocol: Protocol,
        pool: Pubkey,
        user: Pubkey,
        token_a: Pubkey,
        token_b: Pubkey,
        amount_a: u64,
        amount_b: u64,
    ) -> Self {
        Self {
            protocol,
            pool,
            user,
            token_a,
            token_b,
            amount_a,
            amount_b,
            lp_amount: None,
            liquidity_delta: None,
            tick_lower: None,
            tick_upper: None,
            bin_id_lower: None,
            bin_id_upper: None,
            extra: None,
        }
    }

    /// Set LP amount minted.
    #[must_use]
    pub fn with_lp_amount(mut self, lp_amount: u64) -> Self {
        self.lp_amount = Some(lp_amount);
        self
    }

    /// Set liquidity delta (concentrated liquidity protocols).
    #[must_use]
    pub fn with_liquidity_delta(mut self, delta: u128) -> Self {
        self.liquidity_delta = Some(delta);
        self
    }

    /// Set tick range (CLMM).
    #[must_use]
    pub fn with_tick_range(mut self, lower: i32, upper: i32) -> Self {
        self.tick_lower = Some(lower);
        self.tick_upper = Some(upper);
        self
    }

    /// Set bin range (DLMM).
    #[must_use]
    pub fn with_bin_range(mut self, lower: i32, upper: i32) -> Self {
        self.bin_id_lower = Some(lower);
        self.bin_id_upper = Some(upper);
        self
    }

    /// Set extra data.
    #[must_use]
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

/// Liquidity removed from a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityRemoveEvent {
    /// Protocol.
    pub protocol: Protocol,

    /// Pool account address.
    pub pool: Pubkey,

    /// User who removed liquidity.
    pub user: Pubkey,

    /// First token mint.
    pub token_a: Pubkey,

    /// Second token mint.
    pub token_b: Pubkey,

    /// Amount of token A withdrawn.
    pub amount_a: u64,

    /// Amount of token B withdrawn.
    pub amount_b: u64,

    /// LP tokens burned (constant product AMMs).
    pub lp_amount: Option<u64>,

    /// Liquidity units removed (CLMM/DLMM concentrated liquidity).
    pub liquidity_delta: Option<u128>,

    /// Lower tick bound (CLMM only).
    pub tick_lower: Option<i32>,

    /// Upper tick bound (CLMM only).
    pub tick_upper: Option<i32>,

    /// Lower bin ID (DLMM only).
    pub bin_id_lower: Option<i32>,

    /// Upper bin ID (DLMM only).
    pub bin_id_upper: Option<i32>,

    /// Protocol-specific data.
    pub extra: Option<serde_json::Value>,
}

impl LiquidityRemoveEvent {
    /// Create a new liquidity remove event.
    #[must_use]
    pub fn new(
        protocol: Protocol,
        pool: Pubkey,
        user: Pubkey,
        token_a: Pubkey,
        token_b: Pubkey,
        amount_a: u64,
        amount_b: u64,
    ) -> Self {
        Self {
            protocol,
            pool,
            user,
            token_a,
            token_b,
            amount_a,
            amount_b,
            lp_amount: None,
            liquidity_delta: None,
            tick_lower: None,
            tick_upper: None,
            bin_id_lower: None,
            bin_id_upper: None,
            extra: None,
        }
    }

    /// Set LP amount burned.
    #[must_use]
    pub fn with_lp_amount(mut self, lp_amount: u64) -> Self {
        self.lp_amount = Some(lp_amount);
        self
    }

    /// Set liquidity delta (concentrated liquidity protocols).
    #[must_use]
    pub fn with_liquidity_delta(mut self, delta: u128) -> Self {
        self.liquidity_delta = Some(delta);
        self
    }

    /// Set tick range (CLMM).
    #[must_use]
    pub fn with_tick_range(mut self, lower: i32, upper: i32) -> Self {
        self.tick_lower = Some(lower);
        self.tick_upper = Some(upper);
        self
    }

    /// Set bin range (DLMM).
    #[must_use]
    pub fn with_bin_range(mut self, lower: i32, upper: i32) -> Self {
        self.bin_id_lower = Some(lower);
        self.bin_id_upper = Some(upper);
        self
    }

    /// Set extra data.
    #[must_use]
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

// =============================================================================
// Protocol Parser Enum (Enum Dispatch - No Vtable)
// =============================================================================

/// Protocol-specific parser.
///
/// This enum uses static dispatch (no vtable overhead) for parsing.
/// Each variant handles a specific protocol's instructions.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ProtocolParser {
    /// Pumpfun bonding curve parser.
    Pumpfun,
    /// PumpSwap AMM parser.
    PumpSwap,
    /// Raydium V4 AMM parser.
    RaydiumV4,
    /// Raydium CLMM (concentrated liquidity) parser.
    RaydiumClmm,
    /// Raydium CPMM (constant product) parser.
    RaydiumCpmm,
    /// Raydium Launchpad parser.
    RaydiumLaunchpad,
    /// Meteora DLMM (discrete liquidity) parser.
    MeteoraDlmm,
    /// Meteora DBC (dynamic bonding curve) parser.
    MeteoraDbC,
}

impl ProtocolParser {
    /// Get the program ID for this parser.
    #[must_use]
    pub fn program_id(&self) -> Pubkey {
        match self {
            ProtocolParser::Pumpfun => crate::protocols::pumpfun::PROGRAM_ID,
            ProtocolParser::PumpSwap => crate::protocols::pumpswap::PROGRAM_ID,
            ProtocolParser::RaydiumV4 => crate::protocols::raydium_v4::PROGRAM_ID,
            ProtocolParser::RaydiumClmm => crate::protocols::raydium_clmm::PROGRAM_ID,
            ProtocolParser::RaydiumCpmm => crate::protocols::raydium_cpmm::PROGRAM_ID,
            ProtocolParser::RaydiumLaunchpad => crate::protocols::raydium_launchpad::PROGRAM_ID,
            ProtocolParser::MeteoraDlmm => crate::protocols::meteora_dlmm::PROGRAM_ID,
            ProtocolParser::MeteoraDbC => crate::protocols::meteora_dbc::PROGRAM_ID,
        }
    }

    /// Get the protocol enum for this parser.
    #[must_use]
    pub fn protocol(&self) -> Protocol {
        match self {
            ProtocolParser::Pumpfun => Protocol::Pumpfun,
            ProtocolParser::PumpSwap => Protocol::PumpSwap,
            ProtocolParser::RaydiumV4 => Protocol::RaydiumV4,
            ProtocolParser::RaydiumClmm => Protocol::RaydiumClmm,
            ProtocolParser::RaydiumCpmm => Protocol::RaydiumCpmm,
            ProtocolParser::RaydiumLaunchpad => Protocol::RaydiumLaunchpad,
            ProtocolParser::MeteoraDlmm => Protocol::MeteoraDlmm,
            ProtocolParser::MeteoraDbC => Protocol::MeteoraDbC,
        }
    }

    /// Parse an instruction into a protocol event.
    ///
    /// Returns Ok(None) if the discriminator is not recognized.
    ///
    /// Note: For PumpSwap and RaydiumV4, actual swap amounts must be
    /// resolved from CPI transfers via `TransactionContext` (in `tx-context` crate).
    /// This method returns instruction-declared amounts as placeholders.
    pub fn parse(
        &self,
        instruction: &ParsedInstruction,
    ) -> Result<Option<ProtocolEvent>, InstructionParseError> {
        match self {
            ProtocolParser::Pumpfun => pumpfun_parse(instruction),
            ProtocolParser::PumpSwap => pumpswap_parse(instruction),
            ProtocolParser::RaydiumV4 => raydium_v4_parse(instruction),
            ProtocolParser::RaydiumClmm => raydium_clmm_parse(instruction),
            ProtocolParser::MeteoraDlmm => meteora_dlmm_parse(instruction),
            ProtocolParser::RaydiumCpmm => raydium_cpmm_parse(instruction),
            ProtocolParser::RaydiumLaunchpad => raydium_launchpad_parse(instruction),
            ProtocolParser::MeteoraDbC => meteora_dbc_parse(instruction),
        }
    }
}

// =============================================================================
// Protocol Registry
// =============================================================================

/// Registry for protocol instruction parsers.
///
/// Provides O(1) lookup by program ID with enum dispatch (no vtable).
pub struct ProtocolRegistry {
    parsers: HashMap<Pubkey, ProtocolParser>,
}

impl Default for ProtocolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Create a registry with all built-in protocol parsers.
    #[must_use]
    pub fn with_all_protocols() -> Self {
        let mut registry = Self::new();
        registry.register(ProtocolParser::Pumpfun);
        registry.register(ProtocolParser::PumpSwap);
        registry.register(ProtocolParser::RaydiumV4);
        registry.register(ProtocolParser::RaydiumClmm);
        registry.register(ProtocolParser::RaydiumCpmm);
        registry.register(ProtocolParser::RaydiumLaunchpad);
        registry.register(ProtocolParser::MeteoraDlmm);
        registry.register(ProtocolParser::MeteoraDbC);
        registry
    }

    /// Register a protocol parser.
    pub fn register(&mut self, parser: ProtocolParser) {
        let program_id = parser.program_id();
        self.parsers.insert(program_id, parser);
    }

    /// Check if a program ID has a registered parser.
    #[must_use]
    pub fn has_parser(&self, program_id: &Pubkey) -> bool {
        self.parsers.contains_key(program_id)
    }

    /// Get the parser for a program ID.
    #[must_use]
    pub fn get_parser(&self, program_id: &Pubkey) -> Option<ProtocolParser> {
        self.parsers.get(program_id).copied()
    }

    /// Parse an instruction into a protocol event.
    ///
    /// Returns `Ok(None)` if the instruction discriminator is not recognized.
    /// Returns `Err(UnknownProgram)` if no parser is registered for the program.
    pub fn parse(
        &self,
        instruction: &ParsedInstruction,
    ) -> Result<Option<ProtocolEvent>, InstructionParseError> {
        let parser = self.parsers.get(&instruction.program_id).ok_or(
            InstructionParseError::UnknownProgram(instruction.program_id),
        )?;

        parser.parse(instruction)
    }

    /// Try to parse an instruction, returning None for unknown programs.
    pub fn try_parse(
        &self,
        instruction: &ParsedInstruction,
    ) -> Result<Option<ProtocolEvent>, InstructionParseError> {
        match self.parsers.get(&instruction.program_id) {
            Some(parser) => parser.parse(instruction),
            None => Ok(None),
        }
    }

    /// Parse all instructions and collect events.
    pub fn parse_all(
        &self,
        instructions: &[ParsedInstruction],
    ) -> Vec<Result<ProtocolEvent, InstructionParseError>> {
        let mut events = Vec::new();

        for ix in instructions {
            match self.try_parse(ix) {
                Ok(Some(event)) => events.push(Ok(event)),
                Ok(None) => {} // Unknown instruction, skip
                Err(e) => events.push(Err(e)),
            }
        }

        events
    }

    /// Parse all instructions, returning only swap events.
    pub fn parse_swaps(
        &self,
        instructions: &[ParsedInstruction],
    ) -> Vec<Result<SwapEvent, InstructionParseError>> {
        self.parse_all(instructions)
            .into_iter()
            .filter_map(|r| match r {
                Ok(ProtocolEvent::Swap(swap)) => Some(Ok(swap)),
                Ok(
                    ProtocolEvent::TokenCreation(_)
                    | ProtocolEvent::PoolCreation(_)
                    | ProtocolEvent::LiquidityAdd(_)
                    | ProtocolEvent::LiquidityRemove(_),
                ) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    /// Parse all instructions, returning only token creation events.
    pub fn parse_creations(
        &self,
        instructions: &[ParsedInstruction],
    ) -> Vec<Result<TokenCreationEvent, InstructionParseError>> {
        self.parse_all(instructions)
            .into_iter()
            .filter_map(|r| match r {
                Ok(ProtocolEvent::TokenCreation(create)) => Some(Ok(create)),
                Ok(
                    ProtocolEvent::Swap(_)
                    | ProtocolEvent::PoolCreation(_)
                    | ProtocolEvent::LiquidityAdd(_)
                    | ProtocolEvent::LiquidityRemove(_),
                ) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    /// Parse all instructions, returning only pool creation events.
    pub fn parse_pool_creations(
        &self,
        instructions: &[ParsedInstruction],
    ) -> Vec<Result<PoolCreationEvent, InstructionParseError>> {
        self.parse_all(instructions)
            .into_iter()
            .filter_map(|r| match r {
                Ok(ProtocolEvent::PoolCreation(create)) => Some(Ok(create)),
                Ok(
                    ProtocolEvent::Swap(_)
                    | ProtocolEvent::TokenCreation(_)
                    | ProtocolEvent::LiquidityAdd(_)
                    | ProtocolEvent::LiquidityRemove(_),
                ) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    /// Parse all instructions, returning only liquidity add events.
    pub fn parse_liquidity_adds(
        &self,
        instructions: &[ParsedInstruction],
    ) -> Vec<Result<LiquidityAddEvent, InstructionParseError>> {
        self.parse_all(instructions)
            .into_iter()
            .filter_map(|r| match r {
                Ok(ProtocolEvent::LiquidityAdd(add)) => Some(Ok(add)),
                Ok(
                    ProtocolEvent::Swap(_)
                    | ProtocolEvent::TokenCreation(_)
                    | ProtocolEvent::PoolCreation(_)
                    | ProtocolEvent::LiquidityRemove(_),
                ) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    /// Parse all instructions, returning only liquidity remove events.
    pub fn parse_liquidity_removes(
        &self,
        instructions: &[ParsedInstruction],
    ) -> Vec<Result<LiquidityRemoveEvent, InstructionParseError>> {
        self.parse_all(instructions)
            .into_iter()
            .filter_map(|r| match r {
                Ok(ProtocolEvent::LiquidityRemove(remove)) => Some(Ok(remove)),
                Ok(
                    ProtocolEvent::Swap(_)
                    | ProtocolEvent::TokenCreation(_)
                    | ProtocolEvent::PoolCreation(_)
                    | ProtocolEvent::LiquidityAdd(_),
                ) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    /// Get all registered program IDs.
    pub fn registered_programs(&self) -> Vec<Pubkey> {
        self.parsers.keys().copied().collect()
    }
}

// =============================================================================
// Pumpfun Parser Implementation
// =============================================================================

use crate::protocols::pumpfun::{PumpfunInstruction, TRADE_EVENT_DISCRIMINATOR};
use crate::protocols::pumpswap::{
    BuyAccounts as PumpSwapBuyAccounts, CreatePoolAccounts as PumpSwapCreatePoolAccounts,
    DepositAccounts as PumpSwapDepositAccounts, PumpSwapInstruction,
    SellAccounts as PumpSwapSellAccounts, WithdrawAccounts as PumpSwapWithdrawAccounts,
};
use crate::protocols::raydium_clmm::{
    RaydiumClmmInstruction, SwapAccounts as ClmmSwapAccounts, SwapV2Accounts as ClmmSwapV2Accounts,
};
use crate::protocols::raydium_v4::{
    DepositAccounts as RaydiumDepositAccounts, Initialize2Accounts as RaydiumInit2Accounts,
    RaydiumV4Instruction, SwapBaseInAccounts as RaydiumSwapAccounts,
    WithdrawAccounts as RaydiumWithdrawAccounts,
};
// `meteora_dlmm` is being rebuilt against `meteora-dlmm-sdk`. The legacy
// `MeteoraDlmmInstruction` enum + per-ix account structs are gone; the
// parser below is stubbed until step 4 wires the new instructions/
// dispatch back in. See `crates/solana/protocols/src/protocols/meteora_dlmm/`.
use super::traits::FromAccountKeys;
use crate::protocols::meteora_dbc::{
    MeteoraDbcInstruction, SwapAccounts as DbcSwapAccounts, SwapMode as DbcSwapMode,
};
use crate::protocols::raydium_cpmm::{RaydiumCpmmInstruction, SwapAccounts as CpmmSwapAccounts};
use crate::protocols::raydium_launchpad::{
    RaydiumLaunchpadInstruction, SwapAccounts as LaunchpadSwapAccounts,
};

/// Parse a pumpfun instruction using the unified instruction enum.
///
/// This leverages `PumpfunInstruction::try_from_slice()` for parsing
/// and then converts to `ProtocolEvent` for unified handling.
fn pumpfun_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    // Parse instruction using the unified enum
    let pumpfun_ix = PumpfunInstruction::try_from_slice(&instruction.data)?;

    // Convert to ProtocolEvent based on instruction type
    match &pumpfun_ix {
        // v1-layout forms only; the v2 forms use different account indices and
        // this parser hardcodes v1 positions.
        PumpfunInstruction::Buy(_)
        | PumpfunInstruction::BuyExactSolIn(_)
        | PumpfunInstruction::Sell(_) => pumpfun_parse_swap(instruction, &pumpfun_ix),
        PumpfunInstruction::BuyV2(_)
        | PumpfunInstruction::BuyExactQuoteInV2(_)
        | PumpfunInstruction::SellV2(_) => Ok(None),
        PumpfunInstruction::Create(params) => pumpfun_parse_create(instruction, params),
        PumpfunInstruction::CreateV2(params) => pumpfun_parse_create_v2(instruction, params),
        // This legacy registry only speaks swap/creation. Creator-fee movements
        // travel the `chain::extract` path, which is the live one.
        PumpfunInstruction::CollectCreatorFee(_)
        | PumpfunInstruction::CollectCreatorFeeV2(_)
        | PumpfunInstruction::DistributeCreatorFees(_)
        | PumpfunInstruction::DistributeCreatorFeesV2(_) => Ok(None),
    }
}

/// Parse a pumpfun swap (buy or sell) instruction into a SwapEvent.
fn pumpfun_parse_swap(
    instruction: &ParsedInstruction,
    pumpfun_ix: &PumpfunInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let is_buy = pumpfun_ix.is_buy();

    // Get accounts using the known layout
    let mint = *instruction
        .account(2)
        .ok_or(InstructionParseError::MissingAccount(2))?;
    let bonding_curve = *instruction
        .account(3)
        .ok_or(InstructionParseError::MissingAccount(3))?;
    let user = *instruction
        .account(6)
        .ok_or(InstructionParseError::MissingAccount(6))?;

    // Find trade log to get actual amounts
    let log_data = instruction
        .find_data_log_with_discriminator(&TRADE_EVENT_DISCRIMINATOR)
        .ok_or_else(|| {
            if instruction.logs_truncated() {
                InstructionParseError::LogNotFound("trade event (logs truncated)".to_string())
            } else {
                InstructionParseError::LogNotFound("trade event".to_string())
            }
        })?;

    // Parse amounts from log (layout: mint(32) + sol_amount(8) + token_amount(8) + ...)
    if log_data.len() < 32 + 8 + 8 {
        return Err(InstructionParseError::DeserializationFailed(format!(
            "trade event too short: {} bytes",
            log_data.len()
        )));
    }

    let sol_amount = u64::from_le_bytes(log_data[32..40].try_into().unwrap());
    let token_amount = u64::from_le_bytes(log_data[40..48].try_into().unwrap());

    let (token_in, token_out, amount_in, amount_out) = if is_buy {
        (crate::tokens::WSOL, mint, sol_amount, token_amount)
    } else {
        (mint, crate::tokens::WSOL, token_amount, sol_amount)
    };

    Ok(Some(ProtocolEvent::Swap(
        SwapEvent::new(
            Protocol::Pumpfun,
            bonding_curve,
            token_in,
            token_out,
            amount_in,
            amount_out,
        )
        .with_user(user)
        .with_direction(is_buy),
    )))
}

/// Parse a pumpfun create instruction into a TokenCreationEvent.
fn pumpfun_parse_create(
    instruction: &ParsedInstruction,
    params: &crate::protocols::pumpfun::CreateParams,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    // Account layout: 0: mint, 2: bonding_curve, 7: creator
    let mint = *instruction
        .account(0)
        .ok_or(InstructionParseError::MissingAccount(0))?;
    let bonding_curve = *instruction
        .account(2)
        .ok_or(InstructionParseError::MissingAccount(2))?;
    let creator = *instruction
        .account(7)
        .ok_or(InstructionParseError::MissingAccount(7))?;

    Ok(Some(ProtocolEvent::TokenCreation(
        TokenCreationEvent::new(
            Protocol::Pumpfun,
            mint,
            bonding_curve,
            params.name.clone(),
            params.symbol.clone(),
            params.uri.clone(),
        )
        .with_creator(creator),
    )))
}

/// Parse a pumpfun `create_v2` instruction into a TokenCreationEvent.
///
/// v2 has a 16-slot account layout (vs v1's 14): user moved from
/// slot 7 → 5; slot 7 is now `token_program`. Args also include an
/// explicit `creator: Pubkey` which we prefer over the signer.
fn pumpfun_parse_create_v2(
    instruction: &ParsedInstruction,
    params: &crate::protocols::pumpfun::CreateV2Params,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let mint = *instruction
        .account(0)
        .ok_or(InstructionParseError::MissingAccount(0))?;
    let bonding_curve = *instruction
        .account(2)
        .ok_or(InstructionParseError::MissingAccount(2))?;
    let signer = *instruction
        .account(5)
        .ok_or(InstructionParseError::MissingAccount(5))?;

    let creator = if params.creator == solana_program::pubkey::Pubkey::default() {
        signer
    } else {
        params.creator
    };

    Ok(Some(ProtocolEvent::TokenCreation(
        TokenCreationEvent::new(
            Protocol::Pumpfun,
            mint,
            bonding_curve,
            params.name.clone(),
            params.symbol.clone(),
            params.uri.clone(),
        )
        .with_creator(creator),
    )))
}

// =============================================================================
// PumpSwap Parser Implementation
// =============================================================================

/// Parse a PumpSwap instruction.
///
/// Returns protocol events with instruction-declared amounts (slippage bounds).
/// Actual executed amounts must be resolved from CPI transfers via
/// `TransactionContext` at the market-data layer.
fn pumpswap_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let pumpswap_ix = PumpSwapInstruction::try_from_slice(&instruction.data)?;

    match &pumpswap_ix {
        PumpSwapInstruction::BuyExactQuoteIn(params) => {
            let accounts = PumpSwapBuyAccounts::from_account_keys(&instruction.accounts)?;
            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::PumpSwap,
                    accounts.pool,
                    accounts.quote_mint,
                    accounts.base_mint,
                    params.spendable_quote_in,
                    params.min_base_amount_out,
                )
                .with_user(accounts.user)
                .with_direction(true),
            )))
        }
        PumpSwapInstruction::Buy(params) => {
            let accounts = PumpSwapBuyAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::PumpSwap,
                    accounts.pool,
                    accounts.quote_mint, // SOL in
                    accounts.base_mint,  // token out
                    params.max_quote_amount_in,
                    params.base_amount_out,
                )
                .with_user(accounts.user)
                .with_direction(true),
            )))
        }
        PumpSwapInstruction::Sell(params) => {
            let accounts = PumpSwapSellAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::PumpSwap,
                    accounts.pool,
                    accounts.base_mint,  // token in
                    accounts.quote_mint, // SOL out
                    params.base_amount_in,
                    params.min_quote_amount_out,
                )
                .with_user(accounts.user)
                .with_direction(false),
            )))
        }
        PumpSwapInstruction::CreatePool(params) => {
            let accounts = PumpSwapCreatePoolAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::PoolCreation(
                PoolCreationEvent::new(
                    Protocol::PumpSwap,
                    accounts.pool,
                    accounts.base_mint,
                    accounts.quote_mint,
                    params.base_amount_in,
                    params.quote_amount_in,
                )
                .with_creator(accounts.creator)
                .with_lp_mint(accounts.lp_mint),
            )))
        }
        PumpSwapInstruction::Deposit(params) => {
            let accounts = PumpSwapDepositAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::LiquidityAdd(
                LiquidityAddEvent::new(
                    Protocol::PumpSwap,
                    accounts.pool,
                    accounts.user,
                    accounts.base_mint,
                    accounts.quote_mint,
                    params.max_base_amount_in,
                    params.max_quote_amount_in,
                )
                .with_lp_amount(params.lp_token_amount_out),
            )))
        }
        PumpSwapInstruction::Withdraw(params) => {
            let accounts = PumpSwapWithdrawAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::LiquidityRemove(
                LiquidityRemoveEvent::new(
                    Protocol::PumpSwap,
                    accounts.pool,
                    accounts.user,
                    accounts.base_mint,
                    accounts.quote_mint,
                    params.min_base_amount_out,
                    params.min_quote_amount_out,
                )
                .with_lp_amount(params.lp_amount_in),
            )))
        }
        // Fee withdrawals are not swaps; the live `chain::extract` path turns
        // them into `ChainEvent::CreatorFee`.
        PumpSwapInstruction::CollectCoinCreatorFee(_) => Ok(None),
    }
}

// =============================================================================
// Raydium V4 Parser Implementation
// =============================================================================

/// Parse a Raydium V4 instruction.
///
/// Returns protocol events with instruction-declared amounts.
/// For swaps: token mints are not available from accounts alone — only
/// vault token accounts are present. Mint resolution requires `TransactionContext`.
/// For LP ops: actual deposited/withdrawn amounts come from CPI transfers.
fn raydium_v4_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let raydium_ix = RaydiumV4Instruction::try_from_slice(&instruction.data)?;

    match &raydium_ix {
        RaydiumV4Instruction::SwapBaseIn(params) => {
            let accounts = RaydiumSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumV4,
                    accounts.amm_id,
                    accounts.user_source_token_account, // placeholder — not a mint
                    accounts.user_destination_token_account, // placeholder — not a mint
                    params.amount_in,
                    params.minimum_amount_out,
                )
                .with_user(accounts.user_source_owner),
            )))
        }
        RaydiumV4Instruction::SwapBaseOut(params) => {
            let accounts = RaydiumSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumV4,
                    accounts.amm_id,
                    accounts.user_source_token_account, // placeholder — not a mint
                    accounts.user_destination_token_account, // placeholder — not a mint
                    params.max_amount_in,
                    params.amount_out,
                )
                .with_user(accounts.user_source_owner),
            )))
        }
        RaydiumV4Instruction::Initialize2(params) => {
            let accounts = RaydiumInit2Accounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::PoolCreation(
                PoolCreationEvent::new(
                    Protocol::RaydiumV4,
                    accounts.amm_id,
                    accounts.coin_mint,
                    accounts.pc_mint,
                    params.init_coin_amount,
                    params.init_pc_amount,
                )
                .with_creator(accounts.user)
                .with_lp_mint(accounts.lp_mint),
            )))
        }
        RaydiumV4Instruction::Deposit(params) => {
            let accounts = RaydiumDepositAccounts::from_account_keys(&instruction.accounts)?;

            // Note: coin_mint and pc_mint are not directly in Deposit accounts.
            // We use the vault accounts as placeholders — actual mints require
            // CPI enrichment via TransactionContext or pool state lookup.
            Ok(Some(ProtocolEvent::LiquidityAdd(LiquidityAddEvent::new(
                Protocol::RaydiumV4,
                accounts.amm_id,
                accounts.user,
                accounts.coin_vault, // placeholder — vault, not mint
                accounts.pc_vault,   // placeholder — vault, not mint
                params.max_coin_amount,
                params.max_pc_amount,
            ))))
        }
        RaydiumV4Instruction::Withdraw(params) => {
            let accounts = RaydiumWithdrawAccounts::from_account_keys(&instruction.accounts)?;

            // Note: Like Deposit, coin/pc mints are not in Withdraw accounts.
            // Vault accounts used as placeholders. Amount is LP tokens to burn —
            // actual coin/pc amounts come from CPI transfers.
            Ok(Some(ProtocolEvent::LiquidityRemove(
                LiquidityRemoveEvent::new(
                    Protocol::RaydiumV4,
                    accounts.amm_id,
                    accounts.user,
                    accounts.coin_vault, // placeholder — vault, not mint
                    accounts.pc_vault,   // placeholder — vault, not mint
                    0,                   // actual amounts from CPI transfers
                    0,                   // actual amounts from CPI transfers
                )
                .with_lp_amount(params.amount),
            )))
        }
    }
}

// =============================================================================
// Raydium CLMM Parser Implementation
// =============================================================================

fn raydium_clmm_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let clmm_ix = RaydiumClmmInstruction::try_from_slice(&instruction.data)?;

    match &clmm_ix {
        RaydiumClmmInstruction::Swap(params) => {
            let accounts = ClmmSwapAccounts::from_account_keys(&instruction.accounts)?;

            // For CLMM Swap v1, vaults are placeholders for token_in/token_out.
            // Actual mints require CPI enrichment via TransactionContext.
            let (amount_in, amount_out) = if params.is_base_input {
                (params.amount, params.other_amount_threshold)
            } else {
                (params.other_amount_threshold, params.amount)
            };

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumClmm,
                    accounts.pool_state,
                    accounts.input_vault,  // placeholder — vault, not mint
                    accounts.output_vault, // placeholder — vault, not mint
                    amount_in,
                    amount_out,
                )
                .with_user(accounts.payer),
            )))
        }
        RaydiumClmmInstruction::SwapV2(params) => {
            let accounts = ClmmSwapV2Accounts::from_account_keys(&instruction.accounts)?;

            // SwapV2 has explicit mints at indices 11 and 12.
            let (amount_in, amount_out) = if params.is_base_input {
                (params.amount, params.other_amount_threshold)
            } else {
                (params.other_amount_threshold, params.amount)
            };

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumClmm,
                    accounts.pool_state,
                    accounts.input_vault_mint,  // actual mint!
                    accounts.output_vault_mint, // actual mint!
                    amount_in,
                    amount_out,
                )
                .with_user(accounts.payer),
            )))
        }
    }
}

// =============================================================================
// Meteora DLMM Parser Implementation
// =============================================================================

/// Parse a Meteora DLMM instruction.
///
/// Handles swap variants (Swap, SwapExactOut, SwapWithPriceImpact) and
/// liquidity events (AddLiquidity, RemoveLiquidity).
///
/// DLMM has actual mints in swap accounts (token_x_mint at \[6\], token_y_mint at \[7\]).
/// Amounts are instruction-declared (slippage bounds); actual amounts from CPI transfers.
fn meteora_dlmm_parse(
    _instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    // STUB — see `crates/solana/protocols/src/protocols/meteora_dlmm/`.
    // The new ix dispatcher (built on `meteora-dlmm-sdk` layouts) lands
    // in step 4 of the rewrite. Returning `Ok(None)` keeps the registry
    // entry valid: every DLMM instruction parses successfully but emits
    // no `ProtocolEvent`, so the legacy parsing pipeline simply ignores
    // DLMM until the new dispatcher takes over.
    Ok(None)
}

// =============================================================================
// Raydium CPMM Parser Implementation
// =============================================================================

/// Parse a Raydium CPMM instruction.
///
/// CPMM has actual mints in accounts (input_token_mint\[10\], output_token_mint\[11\]).
fn raydium_cpmm_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let cpmm_ix = RaydiumCpmmInstruction::try_from_slice(&instruction.data)?;

    match &cpmm_ix {
        RaydiumCpmmInstruction::SwapBaseInput(params) => {
            let accounts = CpmmSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumCpmm,
                    accounts.pool_state,
                    accounts.input_token_mint,  // actual mint
                    accounts.output_token_mint, // actual mint
                    params.amount_in,
                    params.minimum_amount_out,
                )
                .with_user(accounts.payer),
            )))
        }
        RaydiumCpmmInstruction::SwapBaseOutput(params) => {
            let accounts = CpmmSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumCpmm,
                    accounts.pool_state,
                    accounts.input_token_mint,  // actual mint
                    accounts.output_token_mint, // actual mint
                    params.maximum_amount_in,
                    params.amount_out,
                )
                .with_user(accounts.payer),
            )))
        }
    }
}

// =============================================================================
// Raydium Launchpad Parser Implementation
// =============================================================================

/// Parse a Raydium Launchpad instruction.
///
/// Launchpad has actual mints in accounts (base_token_mint\[9\], quote_token_mint\[10\]).
/// Buy = quote → base (is_buy=true), Sell = base → quote (is_buy=false).
fn raydium_launchpad_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let launch_ix = RaydiumLaunchpadInstruction::try_from_slice(&instruction.data)?;

    match &launch_ix {
        RaydiumLaunchpadInstruction::BuyExactIn(params) => {
            let accounts = LaunchpadSwapAccounts::from_account_keys(&instruction.accounts)?;

            // Buy: quote → base. token_in=quote, token_out=base
            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumLaunchpad,
                    accounts.pool_state,
                    accounts.quote_token_mint, // input (quote)
                    accounts.base_token_mint,  // output (base)
                    params.amount_in,
                    params.minimum_amount_out,
                )
                .with_user(accounts.payer)
                .with_direction(true),
            ))) // is_buy=true
        }
        RaydiumLaunchpadInstruction::BuyExactOut(params) => {
            let accounts = LaunchpadSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumLaunchpad,
                    accounts.pool_state,
                    accounts.quote_token_mint, // input (quote)
                    accounts.base_token_mint,  // output (base)
                    params.maximum_amount_in,
                    params.amount_out,
                )
                .with_user(accounts.payer)
                .with_direction(true),
            )))
        }
        RaydiumLaunchpadInstruction::SellExactIn(params) => {
            let accounts = LaunchpadSwapAccounts::from_account_keys(&instruction.accounts)?;

            // Sell: base → quote. token_in=base, token_out=quote
            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumLaunchpad,
                    accounts.pool_state,
                    accounts.base_token_mint,  // input (base)
                    accounts.quote_token_mint, // output (quote)
                    params.amount_in,
                    params.minimum_amount_out,
                )
                .with_user(accounts.payer)
                .with_direction(false),
            ))) // is_buy=false
        }
        RaydiumLaunchpadInstruction::SellExactOut(params) => {
            let accounts = LaunchpadSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::RaydiumLaunchpad,
                    accounts.pool_state,
                    accounts.base_token_mint,  // input (base)
                    accounts.quote_token_mint, // output (quote)
                    params.maximum_amount_in,
                    params.amount_out,
                )
                .with_user(accounts.payer)
                .with_direction(false),
            )))
        }
    }
}

// =============================================================================
// Meteora DBC Parser Implementation
// =============================================================================

/// Parse a Meteora DBC instruction.
///
/// DBC has actual mints in accounts (base_mint\[7\], quote_mint\[8\]).
/// Supports legacy swap (exact-in) and swap2 (ExactIn/PartialFill/ExactOut).
fn meteora_dbc_parse(
    instruction: &ParsedInstruction,
) -> Result<Option<ProtocolEvent>, InstructionParseError> {
    let dbc_ix = MeteoraDbcInstruction::try_from_slice(&instruction.data)?;

    match &dbc_ix {
        MeteoraDbcInstruction::Swap(params) => {
            let accounts = DbcSwapAccounts::from_account_keys(&instruction.accounts)?;

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::MeteoraDbC,
                    accounts.pool,
                    accounts.base_mint,  // actual mint
                    accounts.quote_mint, // actual mint
                    params.amount_in,
                    params.minimum_amount_out,
                )
                .with_user(accounts.payer),
            )))
        }
        MeteoraDbcInstruction::Swap2(params) => {
            let accounts = DbcSwapAccounts::from_account_keys(&instruction.accounts)?;

            // Swap2 semantics depend on swap_mode:
            // ExactIn/PartialFill: amount_0=input, amount_1=min_output
            // ExactOut: amount_0=desired_output, amount_1=max_input
            let (amount_in, amount_out) = match DbcSwapMode::from_u8(params.swap_mode) {
                Some(DbcSwapMode::ExactIn) | Some(DbcSwapMode::PartialFill) => {
                    (params.amount_0, params.amount_1)
                }
                Some(DbcSwapMode::ExactOut) => (params.amount_1, params.amount_0),
                None => {
                    // Unknown mode — use amount_0 as input, amount_1 as output
                    (params.amount_0, params.amount_1)
                }
            };

            Ok(Some(ProtocolEvent::Swap(
                SwapEvent::new(
                    Protocol::MeteoraDbC,
                    accounts.pool,
                    accounts.base_mint,  // actual mint
                    accounts.quote_mint, // actual mint
                    amount_in,
                    amount_out,
                )
                .with_user(accounts.payer),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_creation() {
        let registry = ProtocolRegistry::new();
        assert!(registry.registered_programs().is_empty());
    }

    #[test]
    fn registry_with_pumpfun() {
        let registry = ProtocolRegistry::with_all_protocols();
        assert!(registry.has_parser(&crate::protocols::pumpfun::PROGRAM_ID));
        assert!(!registry.has_parser(&Pubkey::new_unique()));
    }

    #[test]
    fn registry_unknown_program() {
        let registry = ProtocolRegistry::new();
        let ix = ParsedInstruction::new(Pubkey::new_unique(), vec![], vec![0; 8], 1, 0);

        let result = registry.parse(&ix);
        assert!(matches!(
            result,
            Err(InstructionParseError::UnknownProgram(_))
        ));
    }

    #[test]
    fn registry_try_parse_unknown() {
        let registry = ProtocolRegistry::new();
        let ix = ParsedInstruction::new(Pubkey::new_unique(), vec![], vec![0; 8], 1, 0);

        let result = registry.try_parse(&ix);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn swap_event_creation() {
        let event = SwapEvent::new(
            Protocol::Pumpfun,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            1_000_000_000,
            100_000_000_000,
        )
        .with_fee(12_500_000)
        .with_direction(true);

        assert!(event.is_buy == Some(true));
        assert_eq!(event.fee, Some(12_500_000));
    }

    #[test]
    fn swap_event_effective_price() {
        let event = SwapEvent::new(
            Protocol::Pumpfun,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            1_000_000_000,
            100_000_000,
        );

        let price = event.effective_price(9, 6);
        assert!((price - 100.0).abs() < 0.001);
    }

    #[test]
    fn token_creation_event() {
        let event = TokenCreationEvent::new(
            Protocol::Pumpfun,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            "Test Token".to_string(),
            "TEST".to_string(),
            "https://example.com/meta.json".to_string(),
        )
        .with_creator(Pubkey::new_unique());

        assert_eq!(event.name, "Test Token");
        assert_eq!(event.symbol, "TEST");
        assert!(event.creator.is_some());
    }

    #[test]
    fn protocol_event_accessors() {
        let swap = ProtocolEvent::Swap(SwapEvent::new(
            Protocol::Pumpfun,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            1000,
            2000,
        ));

        assert!(swap.is_swap());
        assert!(!swap.is_token_creation());
        assert!(!swap.is_pool_creation());
        assert!(!swap.is_liquidity_add());
        assert!(!swap.is_liquidity_remove());
        assert!(swap.as_swap().is_some());
        assert!(swap.as_token_creation().is_none());
        assert!(swap.as_pool_creation().is_none());
        assert!(swap.as_liquidity_add().is_none());
        assert!(swap.as_liquidity_remove().is_none());
        assert_eq!(swap.protocol(), Protocol::Pumpfun);
    }

    #[test]
    fn pool_creation_event() {
        let pool = Pubkey::new_unique();
        let token_a = Pubkey::new_unique();
        let token_b = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let lp_mint = Pubkey::new_unique();

        let event = PoolCreationEvent::new(
            Protocol::PumpSwap,
            pool,
            token_a,
            token_b,
            1_000_000,
            500_000_000,
        )
        .with_creator(creator)
        .with_lp_mint(lp_mint);

        assert_eq!(event.protocol, Protocol::PumpSwap);
        assert_eq!(event.pool, pool);
        assert_eq!(event.initial_amount_a, 1_000_000);
        assert_eq!(event.creator, Some(creator));
        assert_eq!(event.lp_mint, Some(lp_mint));

        let wrapped = ProtocolEvent::PoolCreation(event);
        assert!(wrapped.is_pool_creation());
        assert!(wrapped.as_pool_creation().is_some());
        assert_eq!(wrapped.protocol(), Protocol::PumpSwap);
    }

    #[test]
    fn liquidity_add_event() {
        let event = LiquidityAddEvent::new(
            Protocol::PumpSwap,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            500_000,
            1_000_000_000,
        )
        .with_lp_amount(100_000);

        assert_eq!(event.lp_amount, Some(100_000));
        assert!(event.liquidity_delta.is_none());
        assert!(event.tick_lower.is_none());

        let wrapped = ProtocolEvent::LiquidityAdd(event);
        assert!(wrapped.is_liquidity_add());
        assert_eq!(wrapped.protocol(), Protocol::PumpSwap);
    }

    #[test]
    fn liquidity_add_concentrated() {
        let event = LiquidityAddEvent::new(
            Protocol::RaydiumClmm,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            500_000,
            1_000_000_000,
        )
        .with_liquidity_delta(1_000_000_000_000)
        .with_tick_range(-100, 200);

        assert_eq!(event.liquidity_delta, Some(1_000_000_000_000));
        assert_eq!(event.tick_lower, Some(-100));
        assert_eq!(event.tick_upper, Some(200));
        assert!(event.lp_amount.is_none());
    }

    #[test]
    fn liquidity_remove_event() {
        let event = LiquidityRemoveEvent::new(
            Protocol::PumpSwap,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            500_000,
            1_000_000_000,
        )
        .with_lp_amount(100_000);

        assert_eq!(event.lp_amount, Some(100_000));

        let wrapped = ProtocolEvent::LiquidityRemove(event);
        assert!(wrapped.is_liquidity_remove());
        assert_eq!(wrapped.protocol(), Protocol::PumpSwap);
    }

    #[test]
    fn liquidity_remove_dlmm_bins() {
        let event = LiquidityRemoveEvent::new(
            Protocol::MeteoraDlmm,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            500_000,
            1_000_000_000,
        )
        .with_bin_range(-5, 10);

        assert_eq!(event.bin_id_lower, Some(-5));
        assert_eq!(event.bin_id_upper, Some(10));
    }

    #[test]
    fn protocol_parser_enum_dispatch() {
        let parser = ProtocolParser::Pumpfun;
        assert_eq!(parser.program_id(), crate::protocols::pumpfun::PROGRAM_ID);
    }

    #[test]
    fn registry_has_all_protocols() {
        let registry = ProtocolRegistry::with_all_protocols();
        assert!(registry.has_parser(&crate::protocols::pumpfun::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::pumpswap::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::raydium_v4::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::raydium_clmm::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::raydium_cpmm::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::raydium_launchpad::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::meteora_dlmm::PROGRAM_ID));
        assert!(registry.has_parser(&crate::protocols::meteora_dbc::PROGRAM_ID));
        assert_eq!(registry.registered_programs().len(), 8);
    }

    #[test]
    fn pumpswap_parser_protocol() {
        let parser = ProtocolParser::PumpSwap;
        assert_eq!(parser.program_id(), crate::protocols::pumpswap::PROGRAM_ID);
        assert_eq!(parser.protocol(), Protocol::PumpSwap);
    }

    #[test]
    fn raydium_v4_parser_protocol() {
        let parser = ProtocolParser::RaydiumV4;
        assert_eq!(
            parser.program_id(),
            crate::protocols::raydium_v4::PROGRAM_ID
        );
        assert_eq!(parser.protocol(), Protocol::RaydiumV4);
    }

    #[test]
    fn pumpswap_parse_buy() {
        use crate::protocols::pumpswap::{BUY_DISCRIMINATOR, PROGRAM_ID};

        // Build instruction data: 8-byte disc + 8-byte base_amount_out + 8-byte max_quote_amount_in
        let mut data = Vec::from(BUY_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // base_amount_out
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // max_quote_amount_in

        // 23 accounts for Buy
        let accounts: Vec<Pubkey> = (0..23).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = pumpswap_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::PumpSwap);
                assert_eq!(swap.pool, accounts[0]); // pool
                assert_eq!(swap.token_in, accounts[4]); // quote_mint (SOL)
                assert_eq!(swap.token_out, accounts[3]); // base_mint
                assert_eq!(swap.amount_in, 500_000_000); // max_quote_amount_in
                assert_eq!(swap.amount_out, 1_000_000); // base_amount_out
                assert_eq!(swap.user, Some(accounts[1])); // user
                assert_eq!(swap.is_buy, Some(true));
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn pumpswap_parse_sell() {
        use crate::protocols::pumpswap::{PROGRAM_ID, SELL_DISCRIMINATOR};

        // Build instruction data: 8-byte disc + 8-byte base_amount_in + 8-byte min_quote_amount_out
        let mut data = Vec::from(SELL_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // base_amount_in
        data.extend_from_slice(&400_000_000u64.to_le_bytes()); // min_quote_amount_out

        // 21 accounts for Sell
        let accounts: Vec<Pubkey> = (0..21).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = pumpswap_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::PumpSwap);
                assert_eq!(swap.pool, accounts[0]); // pool
                assert_eq!(swap.token_in, accounts[3]); // base_mint
                assert_eq!(swap.token_out, accounts[4]); // quote_mint (SOL)
                assert_eq!(swap.amount_in, 1_000_000); // base_amount_in
                assert_eq!(swap.amount_out, 400_000_000); // min_quote_amount_out
                assert_eq!(swap.user, Some(accounts[1])); // user
                assert_eq!(swap.is_buy, Some(false));
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_v4_parse_swap_base_in() {
        use crate::protocols::raydium_v4::{PROGRAM_ID, SWAP_BASE_IN_IX};

        // Build instruction data: 1-byte disc + 8-byte amount_in + 8-byte minimum_amount_out
        let mut data = vec![SWAP_BASE_IN_IX];
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // amount_in
        data.extend_from_slice(&900_000_000u64.to_le_bytes()); // minimum_amount_out

        // 18 accounts for SwapBaseIn
        let accounts: Vec<Pubkey> = (0..18).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_v4_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumV4);
                assert_eq!(swap.pool, accounts[1]); // amm_id
                assert_eq!(swap.token_in, accounts[15]); // user_source_token_account
                assert_eq!(swap.token_out, accounts[16]); // user_destination_token_account
                assert_eq!(swap.amount_in, 1_000_000_000);
                assert_eq!(swap.amount_out, 900_000_000);
                assert_eq!(swap.user, Some(accounts[17])); // user_source_owner
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_v4_parse_swap_base_out() {
        use crate::protocols::raydium_v4::SWAP_BASE_OUT_IX;

        let mut data = vec![SWAP_BASE_OUT_IX];
        data.extend_from_slice(&1_100_000_000u64.to_le_bytes()); // max_amount_in
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // amount_out

        // 18 accounts
        let accounts: Vec<Pubkey> = (0..18).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_v4::PROGRAM_ID,
            accounts.clone(),
            data,
            1,
            0,
        );

        let result = raydium_v4_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumV4);
                assert_eq!(swap.amount_in, 1_100_000_000);
                assert_eq!(swap.amount_out, 1_000_000_000);
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_v4_parse_initialize2() {
        use crate::protocols::raydium_v4::{INITIALIZE2_IX, PROGRAM_ID};

        let mut data = vec![INITIALIZE2_IX];
        data.push(255u8); // nonce
        data.extend_from_slice(&1_700_000_000u64.to_le_bytes()); // open_time
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // init_pc_amount
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // init_coin_amount

        // 21 accounts for Initialize2
        let accounts: Vec<Pubkey> = (0..21).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_v4_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::PoolCreation(pool) => {
                assert_eq!(pool.protocol, Protocol::RaydiumV4);
                assert_eq!(pool.pool, accounts[4]); // amm_id
                assert_eq!(pool.token_a, accounts[8]); // coin_mint
                assert_eq!(pool.token_b, accounts[9]); // pc_mint
                assert_eq!(pool.initial_amount_a, 1_000_000); // init_coin_amount
                assert_eq!(pool.initial_amount_b, 500_000_000); // init_pc_amount
                assert_eq!(pool.creator, Some(accounts[17])); // user
                assert_eq!(pool.lp_mint, Some(accounts[7])); // lp_mint
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected PoolCreation"),
        }
    }

    #[test]
    fn raydium_v4_parse_deposit() {
        use crate::protocols::raydium_v4::DEPOSIT_IX;

        let mut data = vec![DEPOSIT_IX];
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // max_coin_amount
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // max_pc_amount
        data.extend_from_slice(&0u64.to_le_bytes()); // base_side

        // 14 accounts for Deposit
        let accounts: Vec<Pubkey> = (0..14).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_v4::PROGRAM_ID,
            accounts.clone(),
            data,
            1,
            0,
        );

        let result = raydium_v4_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::LiquidityAdd(add) => {
                assert_eq!(add.protocol, Protocol::RaydiumV4);
                assert_eq!(add.pool, accounts[1]); // amm_id
                assert_eq!(add.user, accounts[12]); // user
                assert_eq!(add.token_a, accounts[6]); // coin_vault (placeholder)
                assert_eq!(add.token_b, accounts[7]); // pc_vault (placeholder)
                assert_eq!(add.amount_a, 1_000_000); // max_coin_amount
                assert_eq!(add.amount_b, 500_000_000); // max_pc_amount
                assert!(add.lp_amount.is_none()); // not set for Raydium V4
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected LiquidityAdd"),
        }
    }

    #[test]
    fn raydium_v4_parse_withdraw() {
        use crate::protocols::raydium_v4::WITHDRAW_IX;

        let mut data = vec![WITHDRAW_IX];
        data.extend_from_slice(&100_000u64.to_le_bytes()); // amount (LP tokens)

        // 20 accounts for Withdraw
        let accounts: Vec<Pubkey> = (0..20).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_v4::PROGRAM_ID,
            accounts.clone(),
            data,
            1,
            0,
        );

        let result = raydium_v4_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::LiquidityRemove(remove) => {
                assert_eq!(remove.protocol, Protocol::RaydiumV4);
                assert_eq!(remove.pool, accounts[1]); // amm_id
                assert_eq!(remove.user, accounts[16]); // user
                assert_eq!(remove.token_a, accounts[6]); // coin_vault (placeholder)
                assert_eq!(remove.token_b, accounts[7]); // pc_vault (placeholder)
                assert_eq!(remove.amount_a, 0); // actual from CPI
                assert_eq!(remove.amount_b, 0); // actual from CPI
                assert_eq!(remove.lp_amount, Some(100_000)); // LP tokens to burn
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_) => panic!("expected LiquidityRemove"),
        }
    }

    #[test]
    fn raydium_clmm_parse_swap() {
        use crate::protocols::raydium_clmm::{PROGRAM_ID, SWAP_DISCRIMINATOR};

        // 8 disc + 8 amount + 8 threshold + 16 sqrt_price + 1 is_base_input = 41
        let mut data = Vec::from(SWAP_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // amount
        data.extend_from_slice(&900_000u64.to_le_bytes()); // other_amount_threshold
        data.extend_from_slice(&4_295_048_017u128.to_le_bytes()); // sqrt_price_limit_x64
        data.push(1u8); // is_base_input = true

        // 9 fixed accounts (tick arrays follow, but not needed for struct)
        let accounts: Vec<Pubkey> = (0..9).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_clmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumClmm);
                assert_eq!(swap.pool, accounts[2]); // pool_state
                assert_eq!(swap.token_in, accounts[5]); // input_vault (placeholder)
                assert_eq!(swap.token_out, accounts[6]); // output_vault (placeholder)
                assert_eq!(swap.amount_in, 1_000_000_000); // is_base_input=true: amount
                assert_eq!(swap.amount_out, 900_000); // threshold
                assert_eq!(swap.user, Some(accounts[0])); // payer
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_clmm_parse_swap_v2() {
        use crate::protocols::raydium_clmm::{PROGRAM_ID, SWAP_V2_DISCRIMINATOR};

        // 8 disc + 8 amount + 8 threshold + 16 sqrt_price + 1 is_base_input = 41
        let mut data = Vec::from(SWAP_V2_DISCRIMINATOR);
        data.extend_from_slice(&630_000_000u64.to_le_bytes());
        data.extend_from_slice(&5_450_147_723_573u64.to_le_bytes());
        data.extend_from_slice(&4_295_048_017u128.to_le_bytes());
        data.push(1u8); // is_base_input = true

        // 13 fixed accounts
        let accounts: Vec<Pubkey> = (0..13).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_clmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumClmm);
                assert_eq!(swap.pool, accounts[2]); // pool_state
                assert_eq!(swap.token_in, accounts[11]); // input_vault_mint (actual mint!)
                assert_eq!(swap.token_out, accounts[12]); // output_vault_mint (actual mint!)
                assert_eq!(swap.amount_in, 630_000_000);
                assert_eq!(swap.amount_out, 5_450_147_723_573);
                assert_eq!(swap.user, Some(accounts[0])); // payer
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_clmm_parse_swap_base_output() {
        use crate::protocols::raydium_clmm::{PROGRAM_ID, SWAP_DISCRIMINATOR};

        let mut data = Vec::from(SWAP_DISCRIMINATOR);
        data.extend_from_slice(&500_000u64.to_le_bytes()); // amount (output)
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // threshold (max input)
        data.extend_from_slice(&79_226_673_521_066_979_257_578_248_091u128.to_le_bytes());
        data.push(0u8); // is_base_input = false

        let accounts: Vec<Pubkey> = (0..9).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_clmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumClmm);
                // is_base_input=false: amount_in=threshold, amount_out=amount
                assert_eq!(swap.amount_in, 1_000_000_000); // threshold becomes input
                assert_eq!(swap.amount_out, 500_000); // amount becomes output
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_clmm_parse_unknown_discriminator() {
        let data = vec![0xFFu8; 41];
        let accounts: Vec<Pubkey> = (0..13).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_clmm::PROGRAM_ID,
            accounts,
            data,
            1,
            0,
        );

        let result = raydium_clmm_parse(&ix);
        assert!(result.is_err());
    }

    #[test]
    fn pumpswap_parse_create_pool() {
        use crate::protocols::pumpswap::{CREATE_POOL_DISCRIMINATOR, PROGRAM_ID};

        // 8 disc + 2 index + 8 base + 8 quote = 26
        let mut data = Vec::from(CREATE_POOL_DISCRIMINATOR);
        data.extend_from_slice(&1u16.to_le_bytes()); // index
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // base_amount_in
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // quote_amount_in

        // 18 accounts for CreatePool
        let accounts: Vec<Pubkey> = (0..18).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = pumpswap_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::PoolCreation(pool) => {
                assert_eq!(pool.protocol, Protocol::PumpSwap);
                assert_eq!(pool.pool, accounts[0]); // pool
                assert_eq!(pool.token_a, accounts[3]); // base_mint
                assert_eq!(pool.token_b, accounts[4]); // quote_mint
                assert_eq!(pool.initial_amount_a, 1_000_000);
                assert_eq!(pool.initial_amount_b, 500_000_000);
                assert_eq!(pool.creator, Some(accounts[2])); // creator
                assert_eq!(pool.lp_mint, Some(accounts[5])); // lp_mint
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected PoolCreation"),
        }
    }

    #[test]
    fn pumpswap_parse_deposit() {
        use crate::protocols::pumpswap::{DEPOSIT_DISCRIMINATOR, PROGRAM_ID};

        // 8 disc + 8 lp + 8 base + 8 quote = 32
        let mut data = Vec::from(DEPOSIT_DISCRIMINATOR);
        data.extend_from_slice(&100_000u64.to_le_bytes()); // lp_token_amount_out
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // max_base_amount_in
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // max_quote_amount_in

        // 18 accounts for Deposit
        let accounts: Vec<Pubkey> = (0..18).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = pumpswap_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::LiquidityAdd(add) => {
                assert_eq!(add.protocol, Protocol::PumpSwap);
                assert_eq!(add.pool, accounts[0]);
                assert_eq!(add.user, accounts[2]);
                assert_eq!(add.token_a, accounts[3]); // base_mint
                assert_eq!(add.token_b, accounts[4]); // quote_mint
                assert_eq!(add.amount_a, 1_000_000); // max_base_amount_in
                assert_eq!(add.amount_b, 500_000_000); // max_quote_amount_in
                assert_eq!(add.lp_amount, Some(100_000)); // lp_token_amount_out
                assert!(add.liquidity_delta.is_none()); // not CLMM
                assert!(add.tick_lower.is_none());
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected LiquidityAdd"),
        }
    }

    #[test]
    fn pumpswap_parse_withdraw() {
        use crate::protocols::pumpswap::{PROGRAM_ID, WITHDRAW_DISCRIMINATOR};

        // 8 disc + 8 lp + 8 base + 8 quote = 32
        let mut data = Vec::from(WITHDRAW_DISCRIMINATOR);
        data.extend_from_slice(&100_000u64.to_le_bytes()); // lp_amount_in
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // min_base_amount_out
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // min_quote_amount_out

        // 18 accounts for Withdraw
        let accounts: Vec<Pubkey> = (0..18).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = pumpswap_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::LiquidityRemove(remove) => {
                assert_eq!(remove.protocol, Protocol::PumpSwap);
                assert_eq!(remove.pool, accounts[0]);
                assert_eq!(remove.user, accounts[2]);
                assert_eq!(remove.token_a, accounts[3]); // base_mint
                assert_eq!(remove.token_b, accounts[4]); // quote_mint
                assert_eq!(remove.amount_a, 1_000_000); // min_base_amount_out
                assert_eq!(remove.amount_b, 500_000_000); // min_quote_amount_out
                assert_eq!(remove.lp_amount, Some(100_000)); // lp_amount_in
                assert!(remove.liquidity_delta.is_none());
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_) => panic!("expected LiquidityRemove"),
        }
    }

    #[test]
    fn pumpswap_parse_not_enough_accounts() {
        use crate::protocols::pumpswap::{BUY_DISCRIMINATOR, PROGRAM_ID};

        let mut data = Vec::from(BUY_DISCRIMINATOR);
        data.extend_from_slice(&1_000_000u64.to_le_bytes());
        data.extend_from_slice(&500_000_000u64.to_le_bytes());

        // Only 5 accounts (need 23)
        let accounts: Vec<Pubkey> = (0..5).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(PROGRAM_ID, accounts, data, 1, 0);

        let result = pumpswap_parse(&ix);
        assert!(matches!(
            result,
            Err(InstructionParseError::NotEnoughAccounts { .. })
        ));
    }

    #[test]
    fn raydium_v4_parse_unknown_instruction() {
        let data = vec![0xFFu8; 17]; // Unknown instruction index
        let accounts: Vec<Pubkey> = (0..18).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_v4::PROGRAM_ID,
            accounts,
            data,
            1,
            0,
        );

        let result = raydium_v4_parse(&ix);
        assert!(matches!(
            result,
            Err(InstructionParseError::UnknownDiscriminatorVec(_))
        ));
    }

    // DLMM parser tests disabled while the meteora_dlmm/ module is
    // rebuilt against meteora-dlmm-sdk. They come back in step 4 of
    // the rewrite when the new ix dispatcher lands.
    #[cfg(any())]
    #[test]
    fn meteora_dlmm_parse_swap() {
        use crate::protocols::meteora_dlmm::{
            constants::SWAP_DISCRIMINATOR as DLMM_SWAP_DISC, PROGRAM_ID as DLMM_PROGRAM_ID,
        };

        // 8 disc + 8 amount_in + 8 min_amount_out = 24
        let mut data = Vec::from(DLMM_SWAP_DISC);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // amount_in
        data.extend_from_slice(&900_000u64.to_le_bytes()); // min_amount_out

        // 15 accounts for Swap
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DLMM_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dlmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::MeteoraDlmm);
                assert_eq!(swap.pool, accounts[0]); // lb_pair
                assert_eq!(swap.token_in, accounts[6]); // token_x_mint
                assert_eq!(swap.token_out, accounts[7]); // token_y_mint
                assert_eq!(swap.amount_in, 1_000_000_000);
                assert_eq!(swap.amount_out, 900_000);
                assert_eq!(swap.user, Some(accounts[10])); // user
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[cfg(any())]
    #[test]
    fn meteora_dlmm_parse_swap_exact_out() {
        use crate::protocols::meteora_dlmm::{
            constants::SWAP_EXACT_OUT_DISCRIMINATOR as DLMM_SWAP_EO_DISC,
            PROGRAM_ID as DLMM_PROGRAM_ID,
        };

        // 8 disc + 8 max_in_amount + 8 out_amount = 24
        let mut data = Vec::from(DLMM_SWAP_EO_DISC);
        data.extend_from_slice(&1_100_000_000u64.to_le_bytes()); // max_in_amount
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // out_amount

        // 15 accounts
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DLMM_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dlmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::MeteoraDlmm);
                assert_eq!(swap.pool, accounts[0]); // lb_pair
                assert_eq!(swap.amount_in, 1_100_000_000); // max_in_amount
                assert_eq!(swap.amount_out, 1_000_000); // out_amount
                assert_eq!(swap.user, Some(accounts[10]));
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[cfg(any())]
    #[test]
    fn meteora_dlmm_parse_add_liquidity() {
        use crate::protocols::meteora_dlmm::{
            constants::ADD_LIQUIDITY_DISCRIMINATOR as DLMM_ADD_LIQ_DISC,
            PROGRAM_ID as DLMM_PROGRAM_ID,
        };

        // 8 disc + dummy params
        let mut data = Vec::from(DLMM_ADD_LIQ_DISC);
        data.extend_from_slice(&[0u8; 32]); // complex params — not parsed

        // 16 accounts for AddLiquidity
        let accounts: Vec<Pubkey> = (0..16).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DLMM_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dlmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::LiquidityAdd(add) => {
                assert_eq!(add.protocol, Protocol::MeteoraDlmm);
                assert_eq!(add.pool, accounts[1]); // lb_pair
                assert_eq!(add.user, accounts[11]); // sender
                assert_eq!(add.token_a, accounts[7]); // token_x_mint
                assert_eq!(add.token_b, accounts[8]); // token_y_mint
                assert_eq!(add.amount_a, 0); // from CPI
                assert_eq!(add.amount_b, 0); // from CPI
                assert!(add.lp_amount.is_none()); // DLMM doesn't have LP tokens
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected LiquidityAdd"),
        }
    }

    #[cfg(any())]
    #[test]
    fn meteora_dlmm_parse_remove_liquidity() {
        use crate::protocols::meteora_dlmm::{
            constants::REMOVE_LIQUIDITY_DISCRIMINATOR as DLMM_REM_LIQ_DISC,
            PROGRAM_ID as DLMM_PROGRAM_ID,
        };

        let mut data = Vec::from(DLMM_REM_LIQ_DISC);
        data.extend_from_slice(&[0u8; 32]); // complex params — not parsed

        // 16 accounts for RemoveLiquidity
        let accounts: Vec<Pubkey> = (0..16).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DLMM_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dlmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::LiquidityRemove(remove) => {
                assert_eq!(remove.protocol, Protocol::MeteoraDlmm);
                assert_eq!(remove.pool, accounts[1]); // lb_pair
                assert_eq!(remove.user, accounts[11]); // sender
                assert_eq!(remove.token_a, accounts[7]); // token_x_mint
                assert_eq!(remove.token_b, accounts[8]); // token_y_mint
                assert_eq!(remove.amount_a, 0); // from CPI
                assert_eq!(remove.amount_b, 0); // from CPI
            }
            ProtocolEvent::Swap(_)
            | ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_) => panic!("expected LiquidityRemove"),
        }
    }

    #[cfg(any())]
    #[test]
    fn meteora_dlmm_parse_unknown_discriminator() {
        let data = vec![0xFFu8; 24];
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::meteora_dlmm::PROGRAM_ID,
            accounts,
            data,
            1,
            0,
        );

        let result = meteora_dlmm_parse(&ix);
        assert!(result.is_err());
    }

    #[test]
    fn raydium_cpmm_parse_swap_base_input() {
        use crate::protocols::raydium_cpmm::{
            PROGRAM_ID as CPMM_PROGRAM_ID, SWAP_BASE_INPUT_DISCRIMINATOR as CPMM_SBI_DISC,
        };

        // 8 disc + 8 amount_in + 8 minimum_amount_out = 24
        let mut data = Vec::from(CPMM_SBI_DISC);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes());
        data.extend_from_slice(&900_000u64.to_le_bytes());

        // 13 accounts
        let accounts: Vec<Pubkey> = (0..13).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(CPMM_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_cpmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumCpmm);
                assert_eq!(swap.pool, accounts[3]); // pool_state
                assert_eq!(swap.token_in, accounts[10]); // input_token_mint
                assert_eq!(swap.token_out, accounts[11]); // output_token_mint
                assert_eq!(swap.amount_in, 1_000_000_000);
                assert_eq!(swap.amount_out, 900_000);
                assert_eq!(swap.user, Some(accounts[0])); // payer
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_cpmm_parse_swap_base_output() {
        use crate::protocols::raydium_cpmm::{
            PROGRAM_ID as CPMM_PROGRAM_ID, SWAP_BASE_OUTPUT_DISCRIMINATOR as CPMM_SBO_DISC,
        };

        let mut data = Vec::from(CPMM_SBO_DISC);
        data.extend_from_slice(&1_100_000_000u64.to_le_bytes()); // maximum_amount_in
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // amount_out

        let accounts: Vec<Pubkey> = (0..13).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(CPMM_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_cpmm_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumCpmm);
                assert_eq!(swap.pool, accounts[3]);
                assert_eq!(swap.amount_in, 1_100_000_000);
                assert_eq!(swap.amount_out, 1_000_000);
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_cpmm_parse_unknown_discriminator() {
        let data = vec![0xFFu8; 24];
        let accounts: Vec<Pubkey> = (0..13).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_cpmm::PROGRAM_ID,
            accounts,
            data,
            1,
            0,
        );

        let result = raydium_cpmm_parse(&ix);
        assert!(result.is_err());
    }

    #[test]
    fn raydium_launchpad_parse_buy_exact_in() {
        use crate::protocols::raydium_launchpad::{
            BUY_EXACT_IN_DISCRIMINATOR as LP_BEI_DISC, PROGRAM_ID as LP_PROGRAM_ID,
        };

        // 8 disc + 8 amount_in + 8 minimum_amount_out + 8 share_fee_rate = 32
        let mut data = Vec::from(LP_BEI_DISC);
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // amount_in (quote)
        data.extend_from_slice(&900_000u64.to_le_bytes()); // minimum_amount_out (base)
        data.extend_from_slice(&0u64.to_le_bytes()); // share_fee_rate

        // 15 accounts
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(LP_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_launchpad_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumLaunchpad);
                assert_eq!(swap.pool, accounts[4]); // pool_state
                assert_eq!(swap.token_in, accounts[10]); // quote_token_mint (input for buy)
                assert_eq!(swap.token_out, accounts[9]); // base_token_mint (output for buy)
                assert_eq!(swap.amount_in, 500_000_000);
                assert_eq!(swap.amount_out, 900_000);
                assert_eq!(swap.user, Some(accounts[0])); // payer
                assert_eq!(swap.is_buy, Some(true));
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_launchpad_parse_sell_exact_in() {
        use crate::protocols::raydium_launchpad::{
            PROGRAM_ID as LP_PROGRAM_ID, SELL_EXACT_IN_DISCRIMINATOR as LP_SEI_DISC,
        };

        let mut data = Vec::from(LP_SEI_DISC);
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // amount_in (base)
        data.extend_from_slice(&400_000_000u64.to_le_bytes()); // minimum_amount_out (quote)
        data.extend_from_slice(&0u64.to_le_bytes()); // share_fee_rate

        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(LP_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_launchpad_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumLaunchpad);
                assert_eq!(swap.pool, accounts[4]); // pool_state
                assert_eq!(swap.token_in, accounts[9]); // base_token_mint (input for sell)
                assert_eq!(swap.token_out, accounts[10]); // quote_token_mint (output for sell)
                assert_eq!(swap.amount_in, 1_000_000);
                assert_eq!(swap.amount_out, 400_000_000);
                assert_eq!(swap.is_buy, Some(false));
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_launchpad_parse_buy_exact_out() {
        use crate::protocols::raydium_launchpad::{
            BUY_EXACT_OUT_DISCRIMINATOR as LP_BEO_DISC, PROGRAM_ID as LP_PROGRAM_ID,
        };

        let mut data = Vec::from(LP_BEO_DISC);
        data.extend_from_slice(&1_000_000u64.to_le_bytes()); // amount_out (base)
        data.extend_from_slice(&600_000_000u64.to_le_bytes()); // maximum_amount_in (quote)
        data.extend_from_slice(&0u64.to_le_bytes()); // share_fee_rate

        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(LP_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = raydium_launchpad_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::RaydiumLaunchpad);
                assert_eq!(swap.amount_in, 600_000_000); // maximum_amount_in
                assert_eq!(swap.amount_out, 1_000_000); // amount_out
                assert_eq!(swap.is_buy, Some(true));
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn raydium_launchpad_parse_unknown_discriminator() {
        let data = vec![0xFFu8; 32];
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::raydium_launchpad::PROGRAM_ID,
            accounts,
            data,
            1,
            0,
        );

        let result = raydium_launchpad_parse(&ix);
        assert!(result.is_err());
    }

    #[test]
    fn meteora_dbc_parse_swap() {
        use crate::protocols::meteora_dbc::{
            constants::SWAP_DISCRIMINATOR as DBC_SWAP_DISC, PROGRAM_ID as DBC_PROGRAM_ID,
        };

        // 8 disc + 8 amount_in + 8 minimum_amount_out = 24
        let mut data = Vec::from(DBC_SWAP_DISC);
        data.extend_from_slice(&1_000_000_000u64.to_le_bytes()); // amount_in
        data.extend_from_slice(&900_000u64.to_le_bytes()); // minimum_amount_out

        // 15 accounts
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DBC_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dbc_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::MeteoraDbC);
                assert_eq!(swap.pool, accounts[2]); // pool
                assert_eq!(swap.token_in, accounts[7]); // base_mint
                assert_eq!(swap.token_out, accounts[8]); // quote_mint
                assert_eq!(swap.amount_in, 1_000_000_000);
                assert_eq!(swap.amount_out, 900_000);
                assert_eq!(swap.user, Some(accounts[9])); // payer
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn meteora_dbc_parse_swap2_exact_in() {
        use crate::protocols::meteora_dbc::{
            constants::SWAP2_DISCRIMINATOR as DBC_SWAP2_DISC, PROGRAM_ID as DBC_PROGRAM_ID,
        };

        // 8 disc + 8 amount_0 + 8 amount_1 + 1 swap_mode = 25
        let mut data = Vec::from(DBC_SWAP2_DISC);
        data.extend_from_slice(&500_000_000u64.to_le_bytes()); // amount_0 (input)
        data.extend_from_slice(&400_000u64.to_le_bytes()); // amount_1 (min output)
        data.push(0u8); // ExactIn

        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DBC_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dbc_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::MeteoraDbC);
                assert_eq!(swap.pool, accounts[2]);
                assert_eq!(swap.amount_in, 500_000_000); // amount_0
                assert_eq!(swap.amount_out, 400_000); // amount_1
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn meteora_dbc_parse_swap2_exact_out() {
        use crate::protocols::meteora_dbc::{
            constants::SWAP2_DISCRIMINATOR as DBC_SWAP2_DISC, PROGRAM_ID as DBC_PROGRAM_ID,
        };

        let mut data = Vec::from(DBC_SWAP2_DISC);
        data.extend_from_slice(&400_000u64.to_le_bytes()); // amount_0 (desired output)
        data.extend_from_slice(&1_100_000_000u64.to_le_bytes()); // amount_1 (max input)
        data.push(2u8); // ExactOut

        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(DBC_PROGRAM_ID, accounts.clone(), data, 1, 0);

        let result = meteora_dbc_parse(&ix).expect("should parse");
        let event = result.expect("should be Some");

        match event {
            ProtocolEvent::Swap(swap) => {
                assert_eq!(swap.protocol, Protocol::MeteoraDbC);
                // ExactOut: amount_in=amount_1 (max input), amount_out=amount_0 (desired output)
                assert_eq!(swap.amount_in, 1_100_000_000); // max input
                assert_eq!(swap.amount_out, 400_000); // desired output
            }
            ProtocolEvent::TokenCreation(_)
            | ProtocolEvent::PoolCreation(_)
            | ProtocolEvent::LiquidityAdd(_)
            | ProtocolEvent::LiquidityRemove(_) => panic!("expected Swap"),
        }
    }

    #[test]
    fn meteora_dbc_parse_unknown_discriminator() {
        let data = vec![0xFFu8; 25];
        let accounts: Vec<Pubkey> = (0..15).map(|_| Pubkey::new_unique()).collect();

        let ix = ParsedInstruction::new(
            crate::protocols::meteora_dbc::PROGRAM_ID,
            accounts,
            data,
            1,
            0,
        );

        let result = meteora_dbc_parse(&ix);
        assert!(result.is_err());
    }
}
