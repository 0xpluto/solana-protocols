//! Token program abstractions.
//!
//! Handles both SPL Token and Token 2022 programs uniformly.
//!
//! # Well-Known Tokens
//!
//! Common token mints are defined as constants for use across the workspace:
//! - [`WSOL`] - Wrapped SOL
//! - [`USDC`] - USD Coin
//! - [`USDT`] - Tether USD
//!
//! # Token Program Detection
//!
//! Use [`TokenWithProgram::detect`] to auto-detect the token program from
//! an ATA address and its owner.

use serde::{Deserialize, Serialize};
use solana_program::program_error::ProgramError;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_sdk::instruction::Instruction;

// =============================================================================
// Well-Known Token Mints
// =============================================================================

/// Wrapped SOL mint address.
pub const WSOL: Pubkey = solana_program::pubkey!("So11111111111111111111111111111111111111112");

/// USDC mint address (mainnet).
pub const USDC: Pubkey = solana_program::pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

/// USDT mint address (mainnet).
pub const USDT: Pubkey = solana_program::pubkey!("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");

/// BONK mint address (mainnet).
pub const BONK: Pubkey = solana_program::pubkey!("DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263");

/// JUP mint address (mainnet).
pub const JUP: Pubkey = solana_program::pubkey!("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN");

/// RAY mint address (mainnet).
pub const RAY: Pubkey = solana_program::pubkey!("4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R");

// =============================================================================
// Base Token Enum
// =============================================================================

/// Common base tokens used by trading protocols.
///
/// Most protocols trade against SOL or stablecoins. This enum
/// provides quick access to common quote tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BaseToken {
    /// Wrapped SOL (native SOL wrapped as SPL token).
    Sol,
    /// USD Coin stablecoin.
    Usdc,
    /// Tether USD stablecoin.
    Usdt,
}

impl BaseToken {
    /// Get the mint address for this base token.
    #[must_use]
    pub const fn mint(&self) -> Pubkey {
        match self {
            BaseToken::Sol => WSOL,
            BaseToken::Usdc => USDC,
            BaseToken::Usdt => USDT,
        }
    }

    /// Get decimals for this token.
    #[must_use]
    pub const fn decimals(&self) -> u8 {
        match self {
            BaseToken::Sol => 9,
            BaseToken::Usdc => 6,
            BaseToken::Usdt => 6,
        }
    }

    /// Get the token program (all base tokens use SPL Token).
    #[must_use]
    pub const fn program(&self) -> TokenProgram {
        // All well-known base tokens use SPL Token (not Token 2022)
        TokenProgram::SplToken
    }

    /// Convert to TokenWithProgram.
    #[must_use]
    pub const fn as_token(&self) -> TokenWithProgram {
        TokenWithProgram {
            mint: self.mint(),
            program: self.program(),
        }
    }

    /// Try to identify base token from mint address.
    #[must_use]
    pub fn from_mint(mint: &Pubkey) -> Option<Self> {
        if *mint == WSOL {
            Some(BaseToken::Sol)
        } else if *mint == USDC {
            Some(BaseToken::Usdc)
        } else if *mint == USDT {
            Some(BaseToken::Usdt)
        } else {
            None
        }
    }

    /// Check if a mint is a known base token.
    #[must_use]
    pub fn is_base_token(mint: &Pubkey) -> bool {
        Self::from_mint(mint).is_some()
    }
}

impl std::fmt::Display for BaseToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaseToken::Sol => write!(f, "SOL"),
            BaseToken::Usdc => write!(f, "USDC"),
            BaseToken::Usdt => write!(f, "USDT"),
        }
    }
}

// =============================================================================
// Token Program Enum
// =============================================================================

/// Token program type (SPL Token or Token 2022).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenProgram {
    /// Original SPL Token program.
    SplToken,
    /// Token 2022 program with extensions.
    SplToken2022,
}

impl TokenProgram {
    /// Create from program ID pubkey.
    ///
    /// Returns `None` if the pubkey doesn't match either token program.
    #[must_use]
    pub fn from_program_id(id: &Pubkey) -> Option<Self> {
        if *id == spl_token::id() {
            Some(TokenProgram::SplToken)
        } else if *id == spl_token_2022::id() {
            Some(TokenProgram::SplToken2022)
        } else {
            None
        }
    }

    /// Get the program ID pubkey.
    #[must_use]
    pub fn id(&self) -> Pubkey {
        match self {
            TokenProgram::SplToken => spl_token::id(),
            TokenProgram::SplToken2022 => spl_token_2022::id(),
        }
    }

    /// Get the associated token account for a mint and owner.
    #[must_use]
    pub fn associated_token_account(&self, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
        match self {
            TokenProgram::SplToken => {
                spl_associated_token_account::get_associated_token_address(owner, mint)
            }
            TokenProgram::SplToken2022 => {
                spl_associated_token_account::get_associated_token_address_with_program_id(
                    owner,
                    mint,
                    &spl_token_2022::id(),
                )
            }
        }
    }

    /// Deserialize mint account data.
    ///
    /// Handles both SPL Token and Token 2022 mint accounts.
    pub fn deserialize_mint(&self, data: &[u8]) -> Result<spl_token::state::Mint, ProgramError> {
        match self {
            TokenProgram::SplToken => spl_token::state::Mint::unpack(data),
            TokenProgram::SplToken2022 => {
                // Try standard unpack first, then try with extensions
                spl_token::state::Mint::unpack(data).or_else(|_| {
                    let state_with_ext = spl_token_2022::extension::StateWithExtensions::<
                        spl_token_2022::state::Mint,
                    >::unpack(data)?;

                    Ok(spl_token::state::Mint {
                        mint_authority: state_with_ext.base.mint_authority,
                        supply: state_with_ext.base.supply,
                        decimals: state_with_ext.base.decimals,
                        is_initialized: state_with_ext.base.is_initialized,
                        freeze_authority: state_with_ext.base.freeze_authority,
                    })
                })
            }
        }
    }

    /// Get decimals from mint account data.
    pub fn decimals(&self, data: &[u8]) -> Result<u8, ProgramError> {
        self.deserialize_mint(data).map(|mint| mint.decimals)
    }

    /// Get the other token program.
    #[must_use]
    pub fn inverse(&self) -> Self {
        match self {
            TokenProgram::SplToken => TokenProgram::SplToken2022,
            TokenProgram::SplToken2022 => TokenProgram::SplToken,
        }
    }
}

impl From<TokenProgram> for Pubkey {
    fn from(tp: TokenProgram) -> Self {
        tp.id()
    }
}

/// Token mint with its associated program.
///
/// This type bundles a token mint address with knowledge of which token
/// program it belongs to, enabling correct ATA derivation and instruction
/// building for both SPL Token and Token 2022.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenWithProgram {
    /// Token mint address.
    pub mint: Pubkey,
    /// Token program type.
    pub program: TokenProgram,
}

impl TokenWithProgram {
    /// Create for an SPL Token mint.
    #[must_use]
    pub fn spl_token(mint: Pubkey) -> Self {
        Self {
            mint,
            program: TokenProgram::SplToken,
        }
    }

    /// Create for a Token 2022 mint.
    #[must_use]
    pub fn token_2022(mint: Pubkey) -> Self {
        Self {
            mint,
            program: TokenProgram::SplToken2022,
        }
    }

    /// Wrapped SOL (SPL Token).
    #[must_use]
    pub const fn wsol() -> Self {
        Self {
            mint: WSOL,
            program: TokenProgram::SplToken,
        }
    }

    /// Auto-detect token program from ATA, owner, and mint.
    ///
    /// This derives the ATA for both SPL Token and Token 2022 programs
    /// and returns the one that matches the provided ATA address.
    ///
    /// # Arguments
    ///
    /// * `ata` - The known associated token account address
    /// * `owner` - The owner of the token account
    /// * `mint` - The token mint address
    ///
    /// # Returns
    ///
    /// `Some(TokenWithProgram)` if the ATA matches either program's derivation,
    /// `None` if it doesn't match either (might be a non-ATA token account).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let token = TokenWithProgram::detect(&ata_address, &wallet, &mint)
    ///     .expect("Unknown token program");
    /// ```
    #[must_use]
    pub fn detect(ata: &Pubkey, owner: &Pubkey, mint: &Pubkey) -> Option<Self> {
        // Try SPL Token first (more common)
        let spl_ata = TokenProgram::SplToken.associated_token_account(mint, owner);
        if spl_ata == *ata {
            return Some(Self {
                mint: *mint,
                program: TokenProgram::SplToken,
            });
        }

        // Try Token 2022
        let token_2022_ata = TokenProgram::SplToken2022.associated_token_account(mint, owner);
        if token_2022_ata == *ata {
            return Some(Self {
                mint: *mint,
                program: TokenProgram::SplToken2022,
            });
        }

        // Neither matched - could be a non-ATA token account
        None
    }

    /// Check if this is wrapped SOL.
    #[must_use]
    pub fn is_wsol(&self) -> bool {
        self.mint == WSOL
    }

    /// Get the associated token account for an owner.
    #[must_use]
    pub fn associated_token_account(&self, owner: &Pubkey) -> Pubkey {
        self.program.associated_token_account(&self.mint, owner)
    }

    /// Create an idempotent instruction to create an Associated Token Account.
    ///
    /// Uses `create_associated_token_account_idempotent` which succeeds even
    /// if the account already exists.
    #[must_use]
    pub fn create_ata_instruction(&self, owner: &Pubkey, payer: &Pubkey) -> Instruction {
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            payer,
            owner,
            &self.mint,
            &self.program.id(),
        )
    }

    /// Create a close_account instruction.
    ///
    /// Closes the token account and transfers remaining rent lamports to destination.
    /// For WSOL accounts, this also unwraps any remaining SOL balance.
    pub fn close_account_instruction(
        &self,
        account: &Pubkey,
        destination: &Pubkey,
        authority: &Pubkey,
    ) -> Result<Instruction, ProgramError> {
        match self.program {
            TokenProgram::SplToken => spl_token::instruction::close_account(
                &spl_token::id(),
                account,
                destination,
                authority,
                &[],
            ),
            TokenProgram::SplToken2022 => spl_token_2022::instruction::close_account(
                &spl_token_2022::id(),
                account,
                destination,
                authority,
                &[],
            ),
        }
    }

    /// Create a transfer instruction.
    ///
    /// For SPL Token, uses the simple `transfer` instruction.
    /// For Token 2022, uses `transfer_checked` which requires decimals.
    pub fn transfer_instruction(
        &self,
        source: &Pubkey,
        destination: &Pubkey,
        authority: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction, ProgramError> {
        match self.program {
            TokenProgram::SplToken => spl_token::instruction::transfer(
                &spl_token::id(),
                source,
                destination,
                authority,
                &[],
                amount,
            ),
            TokenProgram::SplToken2022 => spl_token_2022::instruction::transfer_checked(
                &spl_token_2022::id(),
                source,
                &self.mint,
                destination,
                authority,
                &[],
                amount,
                decimals,
            ),
        }
    }

    /// Create a simple transfer instruction (SPL Token only).
    ///
    /// For Token 2022, this uses `transfer_checked` with decimals=0.
    /// Prefer `transfer_instruction` with correct decimals for Token 2022.
    pub fn transfer_instruction_simple(
        &self,
        source: &Pubkey,
        destination: &Pubkey,
        authority: &Pubkey,
        amount: u64,
    ) -> Result<Instruction, ProgramError> {
        match self.program {
            TokenProgram::SplToken => spl_token::instruction::transfer(
                &spl_token::id(),
                source,
                destination,
                authority,
                &[],
                amount,
            ),
            TokenProgram::SplToken2022 => {
                // Use transfer_checked with decimals - caller should use transfer_instruction
                // with proper decimals for Token 2022
                spl_token_2022::instruction::transfer_checked(
                    &spl_token_2022::id(),
                    source,
                    &self.mint,
                    destination,
                    authority,
                    &[],
                    amount,
                    0, // Caller should use transfer_instruction with proper decimals
                )
            }
        }
    }
}

/// A token account with owner and balance information.
///
/// This struct facilitates transfers and token account operations by
/// bundling the token info with ownership and balance data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAccount {
    /// The token (mint + program).
    pub token: TokenWithProgram,
    /// Account owner.
    pub owner: Pubkey,
    /// Current balance in smallest units.
    pub balance: u64,
}

/// Byte span of the SPL token-account fields we read: `mint(32) + owner(32) +
/// amount(8)`. Token-2022 accounts share this base layout and append TLV
/// extensions after byte 165, so the gate is a **minimum**, not an equality —
/// the same lesson the PumpSwap pool's `>= 301` size check taught (real
/// accounts carry variable trailing bytes; only the field span is fixed).
pub const TOKEN_ACCOUNT_BASE_LEN: usize = 165;

impl TokenAccount {
    /// Decode from raw account bytes.
    ///
    /// `program` comes from the caller's dispatch context (the account's owner),
    /// not from the data — a token account does not name its own program, so
    /// passing it is what keeps classic-vs-Token-2022 unambiguous.
    ///
    /// Reads only `mint` / `owner` / `amount`; the delegate, state, and
    /// close-authority tail is deliberately not modelled (nothing consumes it,
    /// and an unmodelled field cannot drift).
    ///
    /// # Errors
    ///
    /// Returns `Error::AccountDataTooShort` if `data` is shorter than the
    /// 165-byte base layout.
    pub fn from_account_data(data: &[u8], program: TokenProgram) -> crate::error::Result<Self> {
        if data.len() < TOKEN_ACCOUNT_BASE_LEN {
            return Err(crate::error::Error::AccountDataTooShort {
                expected: TOKEN_ACCOUNT_BASE_LEN,
                actual: data.len(),
            });
        }
        let mint = Pubkey::try_from(&data[0..32])
            .map_err(|_| crate::error::Error::invalid_account_data("token account mint"))?;
        let owner = Pubkey::try_from(&data[32..64])
            .map_err(|_| crate::error::Error::invalid_account_data("token account owner"))?;
        let amount = u64::from_le_bytes(
            data[64..72]
                .try_into()
                .map_err(|_| crate::error::Error::invalid_account_data("token account amount"))?,
        );
        Ok(Self {
            token: TokenWithProgram { mint, program },
            owner,
            balance: amount,
        })
    }

    /// Create a new token account.
    #[must_use]
    pub fn new(token: TokenWithProgram, owner: Pubkey, balance: u64) -> Self {
        Self {
            token,
            owner,
            balance,
        }
    }

    /// Create for an SPL Token mint.
    #[must_use]
    pub fn spl_token(mint: Pubkey, owner: Pubkey, balance: u64) -> Self {
        Self::new(TokenWithProgram::spl_token(mint), owner, balance)
    }

    /// Create for a Token 2022 mint.
    #[must_use]
    pub fn token_2022(mint: Pubkey, owner: Pubkey, balance: u64) -> Self {
        Self::new(TokenWithProgram::token_2022(mint), owner, balance)
    }

    /// Create a WSOL account.
    #[must_use]
    pub fn wsol(owner: Pubkey, balance: u64) -> Self {
        Self::new(TokenWithProgram::wsol(), owner, balance)
    }

    /// Get the mint address.
    #[must_use]
    pub fn mint(&self) -> &Pubkey {
        &self.token.mint
    }

    /// Get the token program.
    #[must_use]
    pub fn program(&self) -> TokenProgram {
        self.token.program
    }

    /// Get the associated token account address.
    #[must_use]
    pub fn ata(&self) -> Pubkey {
        self.token.associated_token_account(&self.owner)
    }

    /// Check if this is a WSOL account.
    #[must_use]
    pub fn is_wsol(&self) -> bool {
        self.token.is_wsol()
    }

    /// Check if balance is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.balance == 0
    }

    /// Check if balance is sufficient for an amount.
    #[must_use]
    pub fn has_balance(&self, amount: u64) -> bool {
        self.balance >= amount
    }

    /// Create an idempotent ATA creation instruction.
    #[must_use]
    pub fn create_ata_instruction(&self, payer: &Pubkey) -> Instruction {
        self.token.create_ata_instruction(&self.owner, payer)
    }

    /// Create a transfer instruction from this account to a destination.
    ///
    /// The authority must sign the transaction.
    pub fn transfer_to(
        &self,
        destination: &Pubkey,
        authority: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction, ProgramError> {
        self.token
            .transfer_instruction(&self.ata(), destination, authority, amount, decimals)
    }

    /// Create a transfer instruction to another TokenAccount.
    pub fn transfer_to_account(
        &self,
        destination: &TokenAccount,
        authority: &Pubkey,
        amount: u64,
        decimals: u8,
    ) -> Result<Instruction, ProgramError> {
        self.transfer_to(&destination.ata(), authority, amount, decimals)
    }

    /// Create a close account instruction.
    ///
    /// Closes the ATA and transfers rent to the destination.
    pub fn close_instruction(
        &self,
        destination: &Pubkey,
        authority: &Pubkey,
    ) -> Result<Instruction, ProgramError> {
        self.token
            .close_account_instruction(&self.ata(), destination, authority)
    }

    /// Update the balance (for tracking purposes).
    pub fn set_balance(&mut self, balance: u64) {
        self.balance = balance;
    }

    /// Add to balance (saturating).
    pub fn add_balance(&mut self, amount: u64) {
        self.balance = self.balance.saturating_add(amount);
    }

    /// Subtract from balance (saturating).
    pub fn sub_balance(&mut self, amount: u64) {
        self.balance = self.balance.saturating_sub(amount);
    }
}

/// Builder for token account operations.
///
/// Useful for building multiple related instructions.
#[derive(Debug, Clone)]
pub struct TokenAccountBuilder {
    /// The token (mint + program).
    pub token: TokenWithProgram,
    /// Account owner.
    pub owner: Pubkey,
    /// Token decimals (needed for Token 2022 transfers).
    pub decimals: u8,
}

impl TokenAccountBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new(token: TokenWithProgram, owner: Pubkey, decimals: u8) -> Self {
        Self {
            token,
            owner,
            decimals,
        }
    }

    /// Create for SPL Token.
    #[must_use]
    pub fn spl_token(mint: Pubkey, owner: Pubkey, decimals: u8) -> Self {
        Self::new(TokenWithProgram::spl_token(mint), owner, decimals)
    }

    /// Create for Token 2022.
    #[must_use]
    pub fn token_2022(mint: Pubkey, owner: Pubkey, decimals: u8) -> Self {
        Self::new(TokenWithProgram::token_2022(mint), owner, decimals)
    }

    /// Create for WSOL (9 decimals).
    #[must_use]
    pub fn wsol(owner: Pubkey) -> Self {
        Self::new(TokenWithProgram::wsol(), owner, 9)
    }

    /// Get the ATA address.
    #[must_use]
    pub fn ata(&self) -> Pubkey {
        self.token.associated_token_account(&self.owner)
    }

    /// Create idempotent ATA creation instruction.
    #[must_use]
    pub fn create_ata_instruction(&self, payer: &Pubkey) -> Instruction {
        self.token.create_ata_instruction(&self.owner, payer)
    }

    /// Create transfer instruction from this account.
    pub fn transfer_from(
        &self,
        destination: &Pubkey,
        authority: &Pubkey,
        amount: u64,
    ) -> Result<Instruction, ProgramError> {
        self.token
            .transfer_instruction(&self.ata(), destination, authority, amount, self.decimals)
    }

    /// Create transfer instruction to this account.
    pub fn transfer_to(
        &self,
        source: &Pubkey,
        authority: &Pubkey,
        amount: u64,
    ) -> Result<Instruction, ProgramError> {
        self.token
            .transfer_instruction(source, &self.ata(), authority, amount, self.decimals)
    }

    /// Create close account instruction.
    pub fn close_instruction(
        &self,
        destination: &Pubkey,
        authority: &Pubkey,
    ) -> Result<Instruction, ProgramError> {
        self.token
            .close_account_instruction(&self.ata(), destination, authority)
    }

    /// Build a TokenAccount with a balance.
    #[must_use]
    pub fn with_balance(&self, balance: u64) -> TokenAccount {
        TokenAccount::new(self.token.clone(), self.owner, balance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_program_from_id() {
        assert_eq!(
            TokenProgram::from_program_id(&spl_token::id()),
            Some(TokenProgram::SplToken)
        );
        assert_eq!(
            TokenProgram::from_program_id(&spl_token_2022::id()),
            Some(TokenProgram::SplToken2022)
        );
        assert_eq!(TokenProgram::from_program_id(&Pubkey::new_unique()), None);
    }

    #[test]
    fn token_program_id_roundtrip() {
        assert_eq!(
            TokenProgram::from_program_id(&TokenProgram::SplToken.id()),
            Some(TokenProgram::SplToken)
        );
        assert_eq!(
            TokenProgram::from_program_id(&TokenProgram::SplToken2022.id()),
            Some(TokenProgram::SplToken2022)
        );
    }

    #[test]
    fn token_program_inverse() {
        assert_eq!(TokenProgram::SplToken.inverse(), TokenProgram::SplToken2022);
        assert_eq!(TokenProgram::SplToken2022.inverse(), TokenProgram::SplToken);
    }

    #[test]
    fn wsol_constant() {
        let wsol = TokenWithProgram::wsol();
        assert!(wsol.is_wsol());
        assert_eq!(wsol.program, TokenProgram::SplToken);
    }

    #[test]
    fn ata_derivation() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        let spl_token = TokenWithProgram::spl_token(mint);
        let token_2022 = TokenWithProgram::token_2022(mint);

        let ata_spl = spl_token.associated_token_account(&owner);
        let ata_2022 = token_2022.associated_token_account(&owner);

        // ATAs for same mint but different programs should differ
        assert_ne!(ata_spl, ata_2022);

        // Should match direct derivation
        assert_eq!(
            ata_spl,
            spl_associated_token_account::get_associated_token_address(&owner, &mint)
        );
        assert_eq!(
            ata_2022,
            spl_associated_token_account::get_associated_token_address_with_program_id(
                &owner,
                &mint,
                &spl_token_2022::id()
            )
        );
    }

    #[test]
    fn token_account_basics() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        let account = TokenAccount::spl_token(mint, owner, 1_000_000);

        assert_eq!(account.mint(), &mint);
        assert_eq!(account.owner, owner);
        assert_eq!(account.balance, 1_000_000);
        assert!(!account.is_empty());
        assert!(account.has_balance(1_000_000));
        assert!(!account.has_balance(1_000_001));
    }

    #[test]
    fn token_account_wsol() {
        let owner = Pubkey::new_unique();
        let account = TokenAccount::wsol(owner, 1_000_000_000);

        assert!(account.is_wsol());
        assert_eq!(account.program(), TokenProgram::SplToken);
    }

    #[test]
    fn token_account_balance_ops() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        let mut account = TokenAccount::spl_token(mint, owner, 1000);

        account.add_balance(500);
        assert_eq!(account.balance, 1500);

        account.sub_balance(200);
        assert_eq!(account.balance, 1300);

        // Saturating operations
        account.sub_balance(2000);
        assert_eq!(account.balance, 0);
        assert!(account.is_empty());

        account.add_balance(u64::MAX);
        account.add_balance(1);
        assert_eq!(account.balance, u64::MAX); // Saturated
    }

    #[test]
    fn token_account_ata_matches_token() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        let token = TokenWithProgram::spl_token(mint);
        let account = TokenAccount::spl_token(mint, owner, 0);

        assert_eq!(token.associated_token_account(&owner), account.ata());
    }

    #[test]
    fn token_account_builder_basics() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        let builder = TokenAccountBuilder::spl_token(mint, owner, 6);

        assert_eq!(builder.decimals, 6);

        let account = builder.with_balance(1_000_000);
        assert_eq!(account.balance, 1_000_000);
        assert_eq!(account.ata(), builder.ata());
    }

    #[test]
    fn token_account_builder_wsol() {
        let owner = Pubkey::new_unique();
        let builder = TokenAccountBuilder::wsol(owner);

        assert_eq!(builder.decimals, 9);
        assert!(builder.token.is_wsol());
    }

    // =========================================================================
    // BaseToken tests
    // =========================================================================

    #[test]
    fn base_token_mints() {
        assert_eq!(BaseToken::Sol.mint(), WSOL);
        assert_eq!(BaseToken::Usdc.mint(), USDC);
        assert_eq!(BaseToken::Usdt.mint(), USDT);
    }

    #[test]
    fn base_token_decimals() {
        assert_eq!(BaseToken::Sol.decimals(), 9);
        assert_eq!(BaseToken::Usdc.decimals(), 6);
        assert_eq!(BaseToken::Usdt.decimals(), 6);
    }

    #[test]
    fn base_token_from_mint() {
        assert_eq!(BaseToken::from_mint(&WSOL), Some(BaseToken::Sol));
        assert_eq!(BaseToken::from_mint(&USDC), Some(BaseToken::Usdc));
        assert_eq!(BaseToken::from_mint(&USDT), Some(BaseToken::Usdt));
        assert_eq!(BaseToken::from_mint(&Pubkey::new_unique()), None);
    }

    #[test]
    fn base_token_is_base_token() {
        assert!(BaseToken::is_base_token(&WSOL));
        assert!(BaseToken::is_base_token(&USDC));
        assert!(!BaseToken::is_base_token(&BONK));
        assert!(!BaseToken::is_base_token(&Pubkey::new_unique()));
    }

    #[test]
    fn base_token_as_token() {
        let sol_token = BaseToken::Sol.as_token();
        assert_eq!(sol_token.mint, WSOL);
        assert_eq!(sol_token.program, TokenProgram::SplToken);
    }

    #[test]
    fn base_token_display() {
        assert_eq!(BaseToken::Sol.to_string(), "SOL");
        assert_eq!(BaseToken::Usdc.to_string(), "USDC");
        assert_eq!(BaseToken::Usdt.to_string(), "USDT");
    }

    // =========================================================================
    // TokenWithProgram::detect tests
    // =========================================================================

    #[test]
    fn detect_spl_token() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        // Derive the SPL Token ATA
        let ata = spl_associated_token_account::get_associated_token_address(&owner, &mint);

        // Detect should find SPL Token
        let detected = TokenWithProgram::detect(&ata, &owner, &mint);
        assert!(detected.is_some());

        let token = detected.unwrap();
        assert_eq!(token.mint, mint);
        assert_eq!(token.program, TokenProgram::SplToken);
    }

    #[test]
    fn detect_token_2022() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();

        // Derive the Token 2022 ATA
        let ata = spl_associated_token_account::get_associated_token_address_with_program_id(
            &owner,
            &mint,
            &spl_token_2022::id(),
        );

        // Detect should find Token 2022
        let detected = TokenWithProgram::detect(&ata, &owner, &mint);
        assert!(detected.is_some());

        let token = detected.unwrap();
        assert_eq!(token.mint, mint);
        assert_eq!(token.program, TokenProgram::SplToken2022);
    }

    #[test]
    fn detect_non_ata_returns_none() {
        let mint = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let random_account = Pubkey::new_unique();

        // A random address that's not an ATA
        let detected = TokenWithProgram::detect(&random_account, &owner, &mint);
        assert!(detected.is_none());
    }

    // =========================================================================
    // Well-known token constants tests
    // =========================================================================

    #[test]
    fn well_known_tokens_are_valid() {
        // Just verify these are valid pubkeys (not all zeros, etc.)
        assert_ne!(WSOL, Pubkey::default());
        assert_ne!(USDC, Pubkey::default());
        assert_ne!(USDT, Pubkey::default());
        assert_ne!(BONK, Pubkey::default());
        assert_ne!(JUP, Pubkey::default());
        assert_ne!(RAY, Pubkey::default());

        // All should be different
        let tokens = [WSOL, USDC, USDT, BONK, JUP, RAY];
        for i in 0..tokens.len() {
            for j in (i + 1)..tokens.len() {
                assert_ne!(
                    tokens[i], tokens[j],
                    "Tokens at {} and {} are the same",
                    i, j
                );
            }
        }
    }
}

// =============================================================================
// Mint-account facts (enrichment decode)
// =============================================================================

/// Everything the mint ACCOUNT itself reports: which token program owns it,
/// decimals, supply — and, for Token-2022 mints carrying the metadata
/// extension, the display fields. Classic SPL mints embed no display fields;
/// read the Metaplex PDA for those ([`crate::metaplex`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintAccountFacts {
    pub program: TokenProgram,
    pub decimals: u8,
    pub supply: u64,
    pub display: Option<crate::metaplex::MintDisplay>,
}

/// Decode a mint account by its owning program. `None` = not a mint of a
/// known token program (caller counts the skip — an unknown program must
/// not error an enrichment sweep).
#[must_use]
pub fn decode_mint_account(owner: &Pubkey, data: &[u8]) -> Option<MintAccountFacts> {
    use spl_token_2022::extension::BaseStateWithExtensions as _;
    match TokenProgram::from_program_id(owner)? {
        TokenProgram::SplToken => {
            use solana_program::program_pack::Pack as _;
            let m = spl_token::state::Mint::unpack(data).ok()?;
            Some(MintAccountFacts {
                program: TokenProgram::SplToken,
                decimals: m.decimals,
                supply: m.supply,
                display: None,
            })
        }
        TokenProgram::SplToken2022 => {
            let st = spl_token_2022::extension::StateWithExtensions::<
                spl_token_2022::state::Mint,
            >::unpack(data)
            .ok()?;
            let display = st
                .get_variable_len_extension::<spl_token_metadata_interface::state::TokenMetadata>()
                .ok()
                .map(|md| crate::metaplex::MintDisplay {
                    name: md.name.trim_end_matches('\0').to_string(),
                    symbol: md.symbol.trim_end_matches('\0').to_string(),
                    uri: md.uri.trim_end_matches('\0').to_string(),
                });
            Some(MintAccountFacts {
                program: TokenProgram::SplToken2022,
                decimals: st.base.decimals,
                supply: st.base.supply,
                display,
            })
        }
    }
}

#[cfg(test)]
mod mint_account_tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn decodes_t22_mint_with_embedded_metadata_from_golden_fixture() {
        let raw = include_str!("../fixtures/spl_token_2022/mint_pepedog.json");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let data = base64::engine::general_purpose::STANDARD
            .decode(v["account_b64"].as_str().unwrap())
            .unwrap();
        let f = decode_mint_account(&spl_token_2022::id(), &data).unwrap();
        assert_eq!(f.program, TokenProgram::SplToken2022);
        assert_eq!(f.decimals, 6);
        // pepedog launched with 2B supply — the mint the fixed-supply
        // assumption would misquote 2x (2026-08-10 finding).
        assert_eq!(f.supply, 2_000_000_000_000_000);
        let d = f.display.expect("T22 metadata extension present");
        assert_eq!(d.symbol, "pepedog");
        assert!(!d.name.is_empty());
    }

    #[test]
    fn unknown_program_is_a_skip_not_an_error() {
        assert!(decode_mint_account(&Pubkey::new_unique(), &[0u8; 82]).is_none());
    }
}
