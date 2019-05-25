//! Fast fused multiply-add operations that gracefully fall-back to unused
//! operations (involving a change in the precision and a slight but not
//! drastic loss in the performance).
use std::ops::{Add, Mul, Neg, Sub};
use packed_simd::f32x4;

/// Implements fused mutliply-add with an unfused fall-back.
///
/// # Examples
///
///     use alt_fp::Fma;
///
///     assert_eq!(2.0f32.fmadd(3.0f32, 5.0f32), 2.0 * 3.0 + 5.0);
///     assert_eq!(2.0f32.fmsub(3.0f32, 5.0f32), 2.0 * 3.0 - 5.0);
///
pub trait Fma:
    Mul<Output = Self> + Add<Output = Self> + Sub<Output = Self> + Clone + Sized + Neg<Output = Self>
{
    /// Fused multiply-add. Computes `(self * a) + b`.
    #[inline]
    fn fmadd(self, a: Self, b: Self) -> Self {
        (self * a) + b
    }

    /// Fused multiply-sub. Computes `(self * a) - b`.
    #[inline]
    fn fmsub(self, a: Self, b: Self) -> Self {
        (self * a) - b
    }

    /// Fused multiply-add assignment. Computes `self += a * b`.
    #[inline]
    fn fmadd_assign(&mut self, a: Self, b: Self) {
        *self = a.fmadd(b, self.clone());
    }

    /// Fused multiply-sub assignment. Computes `self -= a * b`.
    #[inline]
    fn fmsub_assign(&mut self, a: Self, b: Self) {
        *self = a.fmadd(-b, self.clone());
    }
}

impl Fma for f32 {
    #[cfg(target_feature = "fma")]
    #[inline]
    fn fmadd(self, a: Self, b: Self) -> Self {
        self.mul_add(a, b)
    }
    #[cfg(target_feature = "fma")]
    #[inline]
    fn fmsub(self, a: Self, b: Self) -> Self {
        self.mul_add(a, -b)
    }
}

impl Fma for f64 {
    #[cfg(target_feature = "fma")]
    #[inline]
    fn fmadd(self, a: Self, b: Self) -> Self {
        self.mul_add(a, b)
    }
    #[cfg(target_feature = "fma")]
    #[inline]
    fn fmsub(self, a: Self, b: Self) -> Self {
        self.mul_add(a, -b)
    }
}

impl Fma for f32x4 {
    #[inline]
    fn fmadd(self, a: Self, b: Self) -> Self {
        self.mul_adde(a, b)
    }
    #[inline]
    fn fmsub(self, a: Self, b: Self) -> Self {
        self.mul_adde(a, -b)
    }
}

/// Removes parenthesis to prevent a false compiler warning.
#[doc(hidden)]
#[macro_export]
macro_rules! __p {
    (($x:expr)) => ($x);
    ($($x:tt)*) => ($($x)*);
}

/// A macro for writing fused multiply-add operations naturally.
///
/// # Examples
///
///     use alt_fp::fma;
///
///     assert_eq!(fma![2.0f32 * 3.0f32 + 5.0f32], 2.0 * 3.0 + 5.0);
///     assert_eq!(fma![2.0f32 * 3.0f32 - 5.0f32], 2.0 * 3.0 - 5.0);
///
///     let mut x1 = 2.0;
///     let mut x2 = 2.0;
///     fma![x1 += 3.0 * 5.0];
///     x2 += 3.0 * 5.0;
///     assert_eq!(x1, x2);
///
///     fma![x1 -= 3.0 * 5.0];
///     x2 -= 3.0 * 5.0;
///     assert_eq!(x1, x2);
///
/// Each placeholder matches a single token tree. You have to wrap it with
/// parentheses if it includes more than one token:
///
///     # use alt_fp::fma;
///     assert_eq!(fma![([2.0f32][0]) * 3.0f32 - 5.0f32], 2.0 * 3.0 - 5.0);
///
#[macro_export]
macro_rules! fma {
    ($a:tt * $b:tt + $c:tt) => {
        $crate::fma::Fma::fmadd($crate::__p!($a), $crate::__p!($b), $crate::__p!($c))
    };
    ($a:tt * $b:tt - $c:tt) => {
        $crate::fma::Fma::fmsub($crate::__p!($a), $crate::__p!($b), $crate::__p!($c))
    };
    ($c:tt + $a:tt * $b:tt) => {
        $crate::fma::Fma::fmadd($crate::__p!($a), $crate::__p!($b), $crate::__p!($c))
    };
    ($c:tt - $a:tt * $b:tt) => {
        $crate::fma::Fma::fmsub($crate::__p!($a), $crate::__p!($b), $crate::__p!($c))
    };
    ($c:tt += $a:tt * $b:tt) => {
        $crate::fma::Fma::fmadd_assign(&mut $crate::__p!($c), $crate::__p!($a), $crate::__p!($b))
    };
    ($c:tt -= $a:tt * $b:tt) => {
        $crate::fma::Fma::fmsub_assign(&mut $crate::__p!($c), $crate::__p!($a), $crate::__p!($b))
    };
}
