//! What an instruction produces, declared per instruction type.
//!
//! An extractor used to be one function per protocol with a match over every
//! variant. That put "is this a swap" in a match arm, where forgetting to add an
//! arm is silent and where an instruction that produces two events cannot say
//! so. Here the instruction's own params type implements a trait per event kind
//! it produces:
//!
//! ```ignore
//! impl ExtractsSwap for BuyParams { … }
//! impl ExtractsCreatorFee for CollectCreatorFeeParams { … }
//! ```
//!
//! Three things follow. The kind is *declared* rather than discovered, so a type
//! implementing nothing is a stated non-event rather than a missing arm. A type
//! may implement several, so `create_pool` producing both a migration and a pool
//! creation is expressible. And the shared work — skipping event self-CPIs,
//! decoding the instruction, finding the child event, wrapping in
//! [`ChainEvent`](crate::chain::ChainEvent), reporting failures — happens in one
//! place instead of once per protocol.
//!
//! # Why the event is resolved before the trait is called
//!
//! An earlier sketch split this in two: resolve identity, then read amounts.
//! Real code refuses that shape — pumpfun's v2 swaps derive the pool as a PDA of
//! the mint *the event names*, so identity already needs the event. Finding it
//! twice to keep the split tidy would be tidiness at the cost of a second walk
//! over every sibling instruction.

use solana_program::pubkey::Pubkey;

use super::error::ExtractError;
use super::ExtractContext;
use crate::chain::types::{CreatorFee, Migration, Swap, TokenCreation};
use crate::parsing::event::ProtocolEvent;
use crate::parsing::ParsedInstruction;

/// An instruction that moves value between two tokens.
pub trait ExtractsSwap {
    /// The event carrying what actually moved.
    ///
    /// Declared arguments are slippage bounds and must never be read as
    /// executed amounts — the pinned side is a ceiling or a floor, not a fill.
    type Event: ProtocolEvent;

    /// Build the swap from the event the program emitted and the instruction
    /// that caused it.
    ///
    /// # Errors
    ///
    /// The account list does not match, or the event and the accounts disagree
    /// about identity.
    fn swap(
        &self,
        event: &Self::Event,
        ix: &ParsedInstruction,
        ctx: &dyn ExtractContext,
    ) -> Result<Swap, ExtractError>;
}

/// An instruction that launches a token.
///
/// No associated event: both pumpfun create forms carry everything needed in
/// their accounts and arguments.
pub trait ExtractsCreation {
    /// The event carrying what the instruction does not: the supply minted and
    /// the reserves the curve started at.
    type Event: ProtocolEvent;

    /// Build the creation, enriched by the event when the program emitted one.
    ///
    /// `Option`, not required. Requiring it would drop every creation from a
    /// program build that predates the event — the identity and metadata are in
    /// the instruction and are the point; the reserves are a bonus. The three
    /// `Option` fields on [`TokenCreation`] say exactly this: `None` means the
    /// creation was read from the instruction alone.
    ///
    /// # Errors
    ///
    /// The account list does not match the layout this instruction declares.
    fn creation(
        &self,
        event: Option<&Self::Event>,
        ix: &ParsedInstruction,
        ctx: &dyn ExtractContext,
    ) -> Result<TokenCreation, ExtractError>;
}

/// Find an `emit_cpi!` event where absence is routine.
///
/// [`child_event`] treats a missing event as an error, which is right when the
/// instruction always emits one. `CompleteEvent` rides only on the trade that
/// fills the curve, so on every other trade its absence is the normal case and
/// must not be reported as a gap.
///
/// # Errors
///
/// Only [`ExtractError::EventUndecodable`] — a body that carried our
/// discriminator and would not decode is a defect either way.
pub fn optional_child_event<E: ProtocolEvent>(
    ix: &ParsedInstruction,
    all: &[ParsedInstruction],
    program: &Pubkey,
) -> Result<Option<E>, ExtractError> {
    match child_event::<E>(ix, all, program) {
        Ok(e) => Ok(Some(e)),
        Err(ExtractError::EventMissing { .. }) => Ok(None),
        Err(other) => Err(other),
    }
}

/// An instruction that pays out accrued creator fees.
pub trait ExtractsCreatorFee {
    /// The event carrying the amount. These instructions take no arguments, so
    /// without it there is no number to record.
    type Event: ProtocolEvent;

    /// # Errors
    ///
    /// The account list does not match, where this instruction reads one.
    fn creator_fee(
        &self,
        event: &Self::Event,
        ix: &ParsedInstruction,
    ) -> Result<CreatorFee, ExtractError>;
}

/// An instruction that adds or removes pool liquidity.
///
/// The declared arguments are bounds — `max_base_amount_in`, `min_base_amount_out`
/// — never what moved, so the event is required exactly as it is for a swap.
pub trait ExtractsLiquidity {
    /// The event carrying what actually moved.
    type Event: ProtocolEvent;

    /// # Errors
    ///
    /// The account list does not match, or the event and the accounts disagree.
    fn liquidity(
        &self,
        event: &Self::Event,
        ix: &ParsedInstruction,
    ) -> Result<crate::chain::types::Liquidity, ExtractError>;
}

/// An instruction that moves a token from one protocol to another.
///
/// Carries an event like the other kinds do. It did not, and the cost was
/// visible: pumpswap's migration read its amounts from the instruction's
/// arguments and hardcoded `0` for the source pool, because the instruction
/// simply does not carry them. The event does.
pub trait ExtractsMigration {
    /// The event carrying what actually moved.
    type Event: ProtocolEvent;

    /// # Errors
    ///
    /// The account list does not match, or the event and the accounts disagree.
    fn migration(
        &self,
        event: &Self::Event,
        ix: &ParsedInstruction,
    ) -> Result<Migration, ExtractError>;
}

/// Find the `emit_cpi!` event an instruction produced, as a `Result`.
///
/// The distinction the bare `?` used to lose: a body that carries our
/// discriminator and will not decode is a defect in our layout, while no body at
/// all means the program did not emit one. Same `None` before, different fixes.
///
/// # Errors
///
/// [`ExtractError::EventMissing`] when no child carried it,
/// [`ExtractError::EventUndecodable`] when one did and the body was unreadable.
pub fn child_event<E: ProtocolEvent>(
    ix: &ParsedInstruction,
    all: &[ParsedInstruction],
    program: &Pubkey,
) -> Result<E, ExtractError> {
    for child in all
        .iter()
        .filter(|c| c.parent_index == Some(ix.instruction_index) && c.program_id == *program)
    {
        match E::from_event_instruction(&child.data) {
            Ok(Some(ev)) => return Ok(ev),
            // A different event on the same program — routine dispatch.
            Ok(None) => {}
            Err(source) => {
                return Err(ExtractError::EventUndecodable {
                    event: E::NAME,
                    len: child.data.len(),
                    source,
                })
            }
        }
    }
    // Legacy `emit!`: older program builds wrote the payload to the *outer*
    // instruction's `Program data:` log as `[discriminator || body]`, with no
    // envelope and no self-CPI. Both pump programs have shipped both forms, so
    // this is an Anchor-era fact rather than a protocol quirk — dropping it
    // would silently lose every swap from a build that predates `emit_cpi!`.
    if let Some(payload) = ix.find_data_log_with_discriminator(&E::DISCRIMINATOR) {
        return E::from_event_body(payload).map_err(|source| ExtractError::EventUndecodable {
            event: E::NAME,
            len: payload.len(),
            source,
        });
    }

    Err(ExtractError::EventMissing { event: E::NAME })
}

/// Corroborate one fact against two sources, refusing when they disagree.
///
/// Recording an identity we cannot corroborate is the fabricated-success class,
/// so this returns an error rather than warning and continuing — which is what
/// pumpfun's mint check used to do while pumpswap's pool check refused. Two
/// people wrote the same check on different days and reached opposite verdicts;
/// this is the one decision.
///
/// # Errors
///
/// [`ExtractError::Corroboration`] when the two do not match.
pub fn corroborate(
    field: &'static str,
    from_event: &Pubkey,
    from_accounts: &Pubkey,
) -> Result<(), ExtractError> {
    if from_event == from_accounts {
        return Ok(());
    }
    Err(ExtractError::Corroboration {
        field,
        from_event: from_event.to_string(),
        from_accounts: from_accounts.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corroboration_passes_when_the_sources_agree() {
        let a = Pubkey::new_from_array([7; 32]);
        assert!(corroborate("mint", &a, &a).is_ok());
    }

    #[test]
    fn corroboration_names_both_sides_when_they_do_not() {
        let (a, b) = (
            Pubkey::new_from_array([7; 32]),
            Pubkey::new_from_array([8; 32]),
        );
        let err = corroborate("mint", &a, &b).expect_err("must refuse");
        assert_eq!(err.kind(), "corroboration");
        let msg = err.to_string();
        assert!(msg.contains(&a.to_string()) && msg.contains(&b.to_string()));
    }
}
