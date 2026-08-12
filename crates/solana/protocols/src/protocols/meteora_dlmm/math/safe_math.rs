//! Checked-arithmetic helper trait used throughout the DLMM math
//! port. Each method maps to the standard `checked_*` library call;
//! the trait exists so call sites read the same way the on-chain
//! Anchor program does, and so the failure path returns a typed
//! error string we can bubble up unchanged.
//!
//! Ported from the on-chain Meteora DLMM (`lb_clmm`) program, with
//! `solana_sdk::msg!` removed — we do not run inside a BPF VM.

use ruint::aliases::U256;

pub trait SafeMath<T>: Sized {
    fn safe_add(self, rhs: Self) -> Result<Self, &'static str>;
    fn safe_mul(self, rhs: Self) -> Result<Self, &'static str>;
    fn safe_div(self, rhs: Self) -> Result<Self, &'static str>;
    fn safe_rem(self, rhs: Self) -> Result<Self, &'static str>;
    fn safe_sub(self, rhs: Self) -> Result<Self, &'static str>;
    fn safe_shl(self, offset: T) -> Result<Self, &'static str>;
    fn safe_shr(self, offset: T) -> Result<Self, &'static str>;
}

macro_rules! checked_impl {
    ($t:ty, $offset:ty) => {
        impl SafeMath<$offset> for $t {
            #[inline(always)]
            fn safe_add(self, v: $t) -> Result<$t, &'static str> {
                self.checked_add(v).ok_or("LBError::MathOverflow")
            }

            #[inline(always)]
            fn safe_sub(self, v: $t) -> Result<$t, &'static str> {
                self.checked_sub(v).ok_or("LBError::MathOverflow")
            }

            #[inline(always)]
            fn safe_mul(self, v: $t) -> Result<$t, &'static str> {
                self.checked_mul(v).ok_or("LBError::MathOverflow")
            }

            #[inline(always)]
            fn safe_div(self, v: $t) -> Result<$t, &'static str> {
                self.checked_div(v).ok_or("LBError::MathOverflow")
            }

            #[inline(always)]
            fn safe_rem(self, v: $t) -> Result<$t, &'static str> {
                self.checked_rem(v).ok_or("LBError::MathOverflow")
            }

            #[inline(always)]
            fn safe_shl(self, v: $offset) -> Result<$t, &'static str> {
                self.checked_shl(v).ok_or("LBError::MathOverflow")
            }

            #[inline(always)]
            fn safe_shr(self, v: $offset) -> Result<$t, &'static str> {
                self.checked_shr(v).ok_or("LBError::MathOverflow")
            }
        }
    };
}

checked_impl!(u16, u32);
checked_impl!(i32, u32);
checked_impl!(u32, u32);
checked_impl!(u64, u32);
checked_impl!(i64, u32);
checked_impl!(u128, u32);
checked_impl!(i128, u32);
checked_impl!(usize, u32);
checked_impl!(U256, usize);
