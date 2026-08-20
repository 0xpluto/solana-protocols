//! The four account-slot kinds, on the shape that motivated them.
//!
//! Raydium CLMM's `swap_v2` declares 13 accounts and sends more: a tick-array
//! bitmap extension that may or may not be there, then some number of tick
//! arrays. That is one conditional followed by a rest, which is the shape the
//! taxonomy in `solana_protocols::parsing::accounts` exists to express.
//!
//! These structs are deliberately *not* wired into the CLMM protocol module —
//! they pin the macro's behaviour on a real layout, and the real decoder lands
//! when the builder does. Naming them here rather than nowhere is the difference
//! between a tested capability and machinery nobody has run.

use solana_program::pubkey::Pubkey;
use solana_protocols::parsing::accounts::Conditional;
use solana_protocols_macros::AccountMetas;

const PROGRAM: Pubkey = solana_program::pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

/// One conditional, then a rest — CLMM `swap_v2`, abbreviated to the slots that
/// matter for the shape.
#[derive(Debug, Clone, AccountMetas)]
#[accounts(
    program_id = PROGRAM,
    unverified = "a deliberately abbreviated model of CLMM swap_v2, here to pin the \
                  macro's four slot kinds against a real layout shape — not the CLMM \
                  decoder, which does not exist yet and would need its own fixture"
)]
struct SwapV2Accounts {
    #[account(writable, signer)]
    payer: Pubkey,
    #[account(writable)]
    pool_state: Pubkey,
    /// Anchor `optional: true`: keeps its slot, program id as the sentinel.
    #[account(optional)]
    memo_program: Option<Pubkey>,
    /// Past the declared list: absent means the slot does not exist.
    #[account(conditional)]
    bitmap_extension: Conditional,
    /// Everything after that.
    #[account(
        remaining,
        reason = "tick arrays the swap crosses: how many depends on how far the \
                  price moves, which is not knowable from the account list, and each \
                  is the same kind of thing so there is nothing to name individually"
    )]
    tick_arrays: Vec<Pubkey>,
}

fn k(n: u8) -> Pubkey {
    Pubkey::new_from_array([n; 32])
}

/// Absent conditional means the slot is not there; absent optional means the
/// slot holds the program id. The whole point of two types.
#[test]
fn the_two_absences_encode_differently() {
    // Optional absent, conditional absent: 3 slots, the third is the program id.
    let three = vec![k(1), k(2), PROGRAM];
    let a = SwapV2Accounts::from_pubkeys(&three).expect("parses");
    assert_eq!(a.memo_program, None, "program id in the slot means absent");
    assert_eq!(a.bitmap_extension, Conditional::Absent);
    assert!(a.tick_arrays.is_empty());
    // Round trip: the optional's slot comes back, the conditional's does not.
    assert_eq!(a.to_account_metas().len(), 3);

    // Optional present: same three slots, a real key in the third.
    let present = vec![k(1), k(2), k(9)];
    let b = SwapV2Accounts::from_pubkeys(&present).expect("parses");
    assert_eq!(b.memo_program, Some(k(9)));
    assert_eq!(b.to_account_metas().len(), 3);
}

/// A conditional adds a slot only when present, and the rest follows it.
#[test]
fn conditional_and_rest_extend_the_list() {
    let full = vec![k(1), k(2), PROGRAM, k(4), k(5), k(6)];
    let a = SwapV2Accounts::from_pubkeys(&full).expect("parses");
    assert_eq!(a.bitmap_extension, Conditional::Present(k(4)));
    assert_eq!(a.tick_arrays, vec![k(5), k(6)]);
    // Total accounting: every account is somewhere, and nothing was invented.
    assert_eq!(a.to_account_metas().len(), full.len());
    assert_eq!(
        a.to_account_metas()
            .iter()
            .map(|m| m.pubkey)
            .collect::<Vec<_>>(),
        full
    );
}

/// Parsing cannot produce a hole: conditionals are filled from the count, in
/// order. This is why the prefix rule is only checked on the build path.
#[test]
fn parsing_cannot_produce_a_hole() {
    for len in 3..=6 {
        let keys: Vec<Pubkey> = (0..len).map(|i| k(u8::try_from(i).expect("small"))).collect();
        let a = SwapV2Accounts::from_pubkeys(&keys).expect("parses");
        if !a.tick_arrays.is_empty() {
            assert!(
                a.bitmap_extension.is_present(),
                "a rest entry implies every conditional before it"
            );
        }
    }
}

/// Building one is refused, naming both accounts.
#[test]
fn building_a_hole_is_refused() {
    let a = SwapV2Accounts {
        payer: k(1),
        pool_state: k(2),
        memo_program: None,
        bitmap_extension: Conditional::Absent,
        tick_arrays: vec![k(5)],
    };
    // The infallible path still exists for structs without conditionals, so the
    // check has to be on the path a conditional-bearing builder calls.
    let err = a.try_to_account_metas().expect_err("a hole must be refused");
    assert_eq!(err.absent, "bitmap_extension");
    assert_eq!(err.present, "tick_arrays");

    // With the conditional present it builds, and the rest lands after it.
    let ok = SwapV2Accounts {
        bitmap_extension: Conditional::Present(k(4)),
        ..a
    };
    let metas = ok.try_to_account_metas().expect("no hole");
    assert_eq!(
        metas.iter().map(|m| m.pubkey).collect::<Vec<_>>(),
        vec![k(1), k(2), PROGRAM, k(4), k(5)]
    );
}
