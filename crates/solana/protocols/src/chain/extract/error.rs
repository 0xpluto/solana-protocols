//! Why an instruction we recognise failed to become an event.
//!
//! Extraction was the only layer in this pipeline with no instrument. Decode
//! failures land in [`crate::undecoded`]; log attribution keeps counters. But an
//! instruction that decoded fine and then failed to yield an event exited
//! through one of ~14 `warn!` calls and a bare `None` — and the default log
//! filter carries `solana_protocols=error`, so none of those warnings are
//! reachable in normal operation. Eight of them were bare `?` operators with no
//! log at all.
//!
//! `Option` also cannot say which of two very different things happened: "this
//! instruction produces no event of this kind" and "this instruction should have
//! produced one and we could not read it". The first is routine. The second is a
//! swap missing from the tape.
//!
//! # The variants are the observed cases, not a taxonomy
//!
//! Every variant here comes from a real exit in the pumpfun or pumpswap
//! extractor. There is deliberately no `Other`: on-chain data produces edge
//! cases constantly, and a catch-all is where they go to be forgotten. A new
//! shape of surprise should be a new variant, added when it is first seen and
//! counted from then on.
//!
//! There is also no `NotApplicable`. An instruction that produces no event of a
//! given kind simply does not implement that kind's trait, so it is never asked
//! — the routine case stops being error-shaped instead of being an error we
//! agree to ignore.

use crate::parsing::InstructionParseError;

/// Why an instruction we decoded did not become an event.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtractError {
    /// The account list did not match the layout this instruction declares.
    ///
    /// Usually a program that added or reordered a slot. Names the struct so
    /// the fix is findable without reading the extractor.
    #[error("{expected} account layout did not match: {source}")]
    AccountLayout {
        /// The accounts struct that refused the list.
        expected: &'static str,
        /// Why it refused.
        #[source]
        source: InstructionParseError,
    },

    /// This instruction should emit `event` and no child instruction carried it.
    ///
    /// A swap without its event has no amounts, so there is nothing honest to
    /// record — but the *instruction* happened, and that is worth counting.
    #[error("no {event} on any child instruction")]
    EventMissing {
        /// The event we looked for.
        event: &'static str,
    },

    /// A child carried this event's discriminator and the body would not decode.
    ///
    /// Always a defect in our layout, never a foreign event — the discriminator
    /// already matched. Distinct from [`EventMissing`](Self::EventMissing)
    /// because the fixes are different: one is a layout bug, the other is a
    /// program that stopped emitting.
    #[error("{event} body ({len} bytes) carried our discriminator but did not decode: {source}")]
    EventUndecodable {
        /// The event whose discriminator matched.
        event: &'static str,
        /// Body length, which is usually the tell.
        len: usize,
        /// Why borsh refused it.
        #[source]
        source: InstructionParseError,
    },

    /// Two independent sources of the same fact disagree.
    ///
    /// The event's mint against the instruction's accounts, the event's pool
    /// against the outer instruction's, a PDA derived from the event that does
    /// not appear in the account list. Recording an identity we cannot
    /// corroborate is the fabricated-success class, so this refuses.
    #[error("{field} disagrees: event says {from_event}, accounts say {from_accounts}")]
    Corroboration {
        /// Which fact disagrees.
        field: &'static str,
        /// What the event claims.
        from_event: String,
        /// What the instruction's accounts claim.
        from_accounts: String,
    },

    /// We recognise this instruction and have not modelled its event yet.
    ///
    /// Deliberately an error rather than a quiet `None`: pumpswap `deposit` and
    /// `withdraw` are real liquidity events that simply vanish today. Naming
    /// them makes them countable, which is how they get prioritised.
    #[error("{instruction} is decoded but its event is not modelled yet")]
    Unmodelled {
        /// The instruction we can read but not interpret.
        instruction: &'static str,
    },
}

impl ExtractError {
    /// Stable label for counters and log lines.
    ///
    /// A method rather than `Debug` so the metric's cardinality is bounded by
    /// this list — the payloads (which mint, which body length) belong in the
    /// retained sample, not in a label.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AccountLayout { .. } => "account_layout",
            Self::EventMissing { .. } => "event_missing",
            Self::EventUndecodable { .. } => "event_undecodable",
            Self::Corroboration { .. } => "corroboration",
            Self::Unmodelled { .. } => "unmodelled",
        }
    }
}

/// What an extractor produces for one instruction.
///
/// Three outcomes, not two. `Ok(None)` is the routine case — this instruction
/// is not one that produces an event, or is an inner event self-CPI whose parent
/// already produced it — and it must stay distinguishable from `Err`, which is a
/// gap in us.
pub type Extracted = Result<Option<crate::chain::ChainEvent>, ExtractError>;

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

static FAILURES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tally() -> &'static std::sync::Mutex<std::collections::HashMap<(&'static str, &'static str), u64>>
{
    static T: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<(&'static str, &'static str), u64>>,
    > = std::sync::OnceLock::new();
    T.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Record an extraction failure.
///
/// Counted unconditionally, because this is the layer that decides whether a
/// swap reaches the tape and it had no instrument at all. The `warn!` is at
/// `warn` rather than `trace` on purpose, but the counter is what a consumer
/// should read: the default log filter carries `solana_protocols=error`, so no
/// log line here is reachable in normal operation.
pub fn report_extract_failure(
    protocol: &crate::Protocol,
    ix: &crate::parsing::ParsedInstruction,
    e: &ExtractError,
) {
    FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut t) = tally().lock() {
        *t.entry((protocol.short_name(), e.kind())).or_default() += 1;
    }
    tracing::warn!(
        protocol = protocol.short_name(),
        kind = e.kind(),
        ix_index = ix.instruction_index,
        %e,
        "extraction failed on an instruction we decoded"
    );
}

/// Total extraction failures since start.
#[must_use]
pub fn extract_failures() -> u64 {
    FAILURES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Failures broken down by `(protocol, kind)`, which is what says *what to fix*.
#[must_use]
pub fn extract_failure_tally() -> Vec<((&'static str, &'static str), u64)> {
    let mut v: Vec<_> = tally()
        .lock()
        .map(|t| t.iter().map(|(k, n)| (*k, *n)).collect())
        .unwrap_or_default();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant has a distinct label, or the counter merges two failures
    /// that need different fixes.
    #[test]
    fn kinds_are_distinct() {
        let all = [
            ExtractError::AccountLayout {
                expected: "X",
                source: InstructionParseError::DataTooShort,
            },
            ExtractError::EventMissing { event: "E" },
            ExtractError::EventUndecodable {
                event: "E",
                len: 1,
                source: InstructionParseError::DataTooShort,
            },
            ExtractError::Corroboration {
                field: "mint",
                from_event: "a".into(),
                from_accounts: "b".into(),
            },
            ExtractError::Unmodelled { instruction: "i" },
        ];
        let kinds: std::collections::HashSet<_> = all.iter().map(ExtractError::kind).collect();
        assert_eq!(kinds.len(), all.len());
    }

    /// The message has to name the thing that failed, or a counter tells you
    /// something broke without telling you where to look.
    #[test]
    fn messages_name_the_subject() {
        let e = ExtractError::Corroboration {
            field: "pool",
            from_event: "AAA".into(),
            from_accounts: "BBB".into(),
        };
        let msg = e.to_string();
        assert!(
            msg.contains("pool") && msg.contains("AAA") && msg.contains("BBB"),
            "{msg}"
        );
    }
}
