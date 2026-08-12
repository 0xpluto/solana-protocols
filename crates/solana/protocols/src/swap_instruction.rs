//! Which instruction produced a swap — the unit the quote math is constant over.
//!
//! Grading by "buy vs sell" pools instructions whose math genuinely differs.
//! Pumpfun ships **six** swap instructions and PumpSwap **three**, and they do
//! not agree on the most basic question a quote asks: *which amount did the
//! user fix?* `buy` takes a token amount out and a SOL ceiling;
//! `buy_exact_sol_in` takes a SOL amount in and a token floor. Those are
//! inverse operations, and averaging their math into one formula is why a
//! quoter can get close and never get exact.
//!
//! The table below is the single source for both directions — discriminator to
//! instruction and back — so the two cannot disagree. A hand-written reverse
//! lookup is the variant-omission bug this codebase has already paid for.
//!
//! Every discriminator is **derived**, never transcribed: all nine are
//! `sha256("global:<name>")[..8]`, checked against the values observed on
//! chain before this table was written.

use solana_program::pubkey::Pubkey;

use crate::protocols::Protocol;

/// Which side of the trade the user pinned.
///
/// This is a property of the *instruction*, not a runtime flag. Rounding
/// direction follows from it — an exact-out instruction rounds the input up so
/// the pool is never short, an exact-in instruction rounds the output down —
/// so applying one instruction's math to the other is wrong in a way that only
/// shows on small amounts, where the two disagree by more than dust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountSpecified {
    /// The user fixed the input; the pool computes the output.
    ExactIn,
    /// The user fixed the output; the pool computes the required input.
    ExactOut,
}

macro_rules! swap_instructions {
    ($( $variant:ident => $proto:ident, $name:literal, $kind:ident );+ $(;)?) => {
        /// A specific swap instruction on a specific program.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[non_exhaustive]
        pub enum SwapInstruction {
            $( #[doc = concat!("`", $name, "`")] $variant, )+
            /// A swap instruction we have not catalogued. Carries the
            /// discriminator so it can be identified from recorded data
            /// rather than re-observed — the same reason `TradingPlatform`
            /// keeps the program id of a router it cannot name.
            Unknown([u8; 8]),
        }

        impl SwapInstruction {
            /// Every catalogued instruction. Completeness is pinned by a test
            /// with an exhaustive match, so adding a variant without adding it
            /// here fails to compile.
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

            /// The 8-byte Anchor discriminator, derived at compile time.
            #[must_use]
            pub const fn discriminator(self) -> Option<[u8; 8]> {
                match self {
                    $( Self::$variant => Some(
                        solana_protocols_macros::anchor_instruction_discriminator!($name)
                    ), )+
                    Self::Unknown(_) => None,
                }
            }

            /// The program this instruction belongs to.
            #[must_use]
            pub const fn protocol(self) -> Option<Protocol> {
                match self {
                    $( Self::$variant => Some(Protocol::$proto), )+
                    Self::Unknown(_) => None,
                }
            }

            /// Which amount the user pinned — see [`AmountSpecified`].
            #[must_use]
            pub const fn amount_specified(self) -> Option<AmountSpecified> {
                match self {
                    $( Self::$variant => Some(AmountSpecified::$kind), )+
                    Self::Unknown(_) => None,
                }
            }

            /// Stable name for grouping and reports.
            #[must_use]
            pub fn name(self) -> String {
                match self {
                    $( Self::$variant => $name.to_string(), )+
                    Self::Unknown(d) => format!("unknown:{}", hex8(&d)),
                }
            }

            /// Resolve from the owning program and the instruction's first 8
            /// bytes. Never fabricates: an uncatalogued instruction comes back
            /// as [`Unknown`](Self::Unknown), not as a nearby variant.
            #[must_use]
            pub fn from_discriminator(protocol: Protocol, data: &[u8]) -> Self {
                let Some(head) = data.first_chunk::<8>() else {
                    return Self::Unknown([0; 8]);
                };
                for candidate in Self::ALL {
                    if candidate.protocol() == Some(protocol)
                        && candidate.discriminator().as_ref() == Some(head)
                    {
                        return *candidate;
                    }
                }
                Self::Unknown(*head)
            }
        }
    };
}

swap_instructions! {
    // Pumpfun. `buy` pins the token amount out; the `exact_*_in` forms pin the
    // SOL/quote side. The v2 forms share their sibling's account layout and
    // differ in discriminator and argument semantics only.
    PumpfunBuy               => Pumpfun,  "buy",                   ExactOut;
    PumpfunBuyV2             => Pumpfun,  "buy_v2",                ExactOut;
    PumpfunBuyExactSolIn     => Pumpfun,  "buy_exact_sol_in",      ExactIn;
    PumpfunBuyExactQuoteInV2 => Pumpfun,  "buy_exact_quote_in_v2", ExactIn;
    PumpfunSell              => Pumpfun,  "sell",                  ExactIn;
    PumpfunSellV2            => Pumpfun,  "sell_v2",               ExactIn;

    // PumpSwap. Same split: `buy` pins base out, `buy_exact_quote_in` pins
    // quote in, `sell` pins base in.
    PumpSwapBuy              => PumpSwap, "buy",                   ExactOut;
    PumpSwapBuyExactQuoteIn  => PumpSwap, "buy_exact_quote_in",    ExactIn;
    PumpSwapSell             => PumpSwap, "sell",                  ExactIn;
}

fn hex8(d: &[u8; 8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The program that owns a swap instruction, for callers holding only a
/// program id.
#[must_use]
pub fn resolve(program: &Pubkey, data: &[u8]) -> SwapInstruction {
    match Protocol::from_program_id(program) {
        Some(p) => SwapInstruction::from_discriminator(p, data),
        None => data
            .first_chunk::<8>()
            .map_or(SwapInstruction::Unknown([0; 8]), |h| {
                SwapInstruction::Unknown(*h)
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant is reachable from its own discriminator, and the table is
    /// complete: the exhaustive match fails to compile when a variant is added
    /// without being listed.
    #[test]
    fn every_instruction_round_trips_and_the_list_is_complete() {
        fn listed(i: SwapInstruction) -> bool {
            match i {
                SwapInstruction::PumpfunBuy
                | SwapInstruction::PumpfunBuyV2
                | SwapInstruction::PumpfunBuyExactSolIn
                | SwapInstruction::PumpfunBuyExactQuoteInV2
                | SwapInstruction::PumpfunSell
                | SwapInstruction::PumpfunSellV2
                | SwapInstruction::PumpSwapBuy
                | SwapInstruction::PumpSwapBuyExactQuoteIn
                | SwapInstruction::PumpSwapSell => true,
                SwapInstruction::Unknown(_) => false,
            }
        }
        assert_eq!(SwapInstruction::ALL.len(), 9);
        for ix in SwapInstruction::ALL {
            assert!(listed(*ix), "{ix:?} missing from the completeness match");
            let d = ix.discriminator().expect("catalogued");
            let p = ix.protocol().expect("catalogued");
            assert_eq!(SwapInstruction::from_discriminator(p, &d), *ix);
            assert!(ix.amount_specified().is_some());
        }
    }

    /// The values this table generates must equal what the chain actually
    /// carries. Pinned against bytes observed on mainnet 2026-08-11, so a
    /// rename that silently changes a derivation cannot pass.
    #[test]
    fn derived_discriminators_match_the_chain() {
        for (ix, want) in [
            (
                SwapInstruction::PumpfunBuy,
                [102, 6, 61, 18, 1, 218, 235, 234],
            ),
            (
                SwapInstruction::PumpfunBuyV2,
                [184, 23, 238, 97, 103, 197, 211, 61],
            ),
            (
                SwapInstruction::PumpfunBuyExactSolIn,
                [56, 252, 116, 8, 158, 223, 205, 95],
            ),
            (
                SwapInstruction::PumpfunBuyExactQuoteInV2,
                [194, 171, 28, 70, 104, 77, 91, 47],
            ),
            (
                SwapInstruction::PumpfunSell,
                [51, 230, 133, 164, 1, 127, 131, 173],
            ),
            (
                SwapInstruction::PumpfunSellV2,
                [93, 246, 130, 60, 231, 233, 64, 178],
            ),
            (
                SwapInstruction::PumpSwapBuyExactQuoteIn,
                [198, 46, 21, 82, 180, 217, 232, 112],
            ),
        ] {
            assert_eq!(ix.discriminator(), Some(want), "{ix:?}");
        }
    }

    /// Pumpfun `buy` and `buy_exact_sol_in` pin opposite sides. Collapsing
    /// them into one "buy" bucket is what this type exists to prevent.
    #[test]
    fn the_same_side_can_pin_opposite_amounts() {
        assert_eq!(
            SwapInstruction::PumpfunBuy.amount_specified(),
            Some(AmountSpecified::ExactOut)
        );
        assert_eq!(
            SwapInstruction::PumpfunBuyExactSolIn.amount_specified(),
            Some(AmountSpecified::ExactIn)
        );
    }

    /// An uncatalogued instruction keeps its discriminator instead of being
    /// snapped to a neighbour — identifiable later from recorded data.
    #[test]
    fn an_uncatalogued_instruction_is_preserved_not_guessed() {
        let d = [9u8; 8];
        let ix = SwapInstruction::from_discriminator(Protocol::Pumpfun, &d);
        assert_eq!(ix, SwapInstruction::Unknown(d));
        assert_eq!(ix.protocol(), None);
        assert_eq!(ix.amount_specified(), None);
        assert!(ix.name().starts_with("unknown:"));
    }
}

#[cfg(test)]
mod parser_agreement {
    use super::*;
    use crate::protocols::pumpfun::PumpfunInstruction;
    use crate::protocols::pumpswap::instructions::PumpSwapInstruction;

    /// Every swap instruction this table names must actually parse.
    ///
    /// This is the seam that let five instructions go silently undecoded: the
    /// table said they existed, the parsers had never heard of them, and an
    /// unparsed instruction produces no row rather than an error — so the
    /// swaps simply vanished from the tape with nothing counting them.
    ///
    /// Adding a row to the table without teaching the parser now fails here.
    #[test]
    fn every_catalogued_instruction_parses() {
        // Two u64 params — the shape every swap instruction on both programs
        // uses. Enough for the discriminator dispatch under test.
        let body = [0u8; 16];
        for ix in SwapInstruction::ALL {
            let mut data = ix.discriminator().expect("catalogued").to_vec();
            data.extend_from_slice(&body);
            match ix.protocol().expect("catalogued") {
                crate::protocols::Protocol::Pumpfun => {
                    PumpfunInstruction::try_from_slice(&data)
                        .unwrap_or_else(|e| panic!("{} must parse: {e:?}", ix.name()));
                }
                crate::protocols::Protocol::PumpSwap => {
                    PumpSwapInstruction::try_from_slice(&data)
                        .unwrap_or_else(|e| panic!("{} must parse: {e:?}", ix.name()));
                }
                other => panic!("{other:?} has no parser wired for {}", ix.name()),
            }
        }
    }
}
