//! Fast maximum/minimum value functions for floating-point types.
#[cfg(feature = "packed_simd")]
use packed_simd::{f32x4, f32x8, f64x2, f64x4};

#[cfg(target_feature = "sse")]
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_feature = "sse")]
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use std::mem::transmute;

/// Implements fast maximum/minimum value functions for floating-point types.
///
/// # Implementation notes
///
/// The method defintions of `FloatOrd` are modeled after x86's `minss`/`maxss`
/// instructions so that they can be efficiently implemented on x86 processors.
///
/// # Examples
///
///     use alt_fp::FloatOrd;
///
///     assert_eq!(1.0.fmin(2.0), 1.0);
///     assert_eq!(2.0.fmin(1.0), 1.0);
///
///     assert_eq!(1.0.fmax(2.0), 2.0);
///     assert_eq!(2.0.fmax(1.0), 2.0);
///
/// They handle NaN differently from `<f32>::{min, max}`:
///
///     # use alt_fp::FloatOrd;
///     assert!(<f32>::from_bits(0x7f801234).is_nan() == true);
///     assert!(<f32>::from_bits(0x40000000).is_nan() == false);
///
///     assert_eq!(<f32>::from_bits(0x7f801234)
///         .fmin(<f32>::from_bits(0x40000000)).to_bits(), 0x40000000);
///     assert_eq!(<f32>::from_bits(0x40000000)
///         .fmax(<f32>::from_bits(0x7f801234)).to_bits(), 0x7f801234);
///
///     // Compare the above with:
///     assert_eq!(<f32>::from_bits(0x7f801234)
///         .min(<f32>::from_bits(0x40000000)).to_bits(), 0x40000000);
///     assert_eq!(<f32>::from_bits(0x40000000)
///         .max(<f32>::from_bits(0x7f801234)).to_bits(), 0x40000000);
///
pub trait FloatOrd {
    /// Compute the minimum value of `self` and `x`. Returns `x` if any of the
    /// operands are NaN.
    fn fmin(self, x: Self) -> Self
    where
        Self: Sized;

    /// Compute the maximum value of `self` and `x`. Returns `x` if any of the
    /// operands are NaN.
    fn fmax(self, x: Self) -> Self
    where
        Self: Sized;
}

impl FloatOrd for f32 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        if self < x { self } else { x }
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        if self > x { self } else { x }
    }
}

impl FloatOrd for f64 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        if self < x { self } else { x }
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        if self > x { self } else { x }
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(not(target_feature = "sse"))]
impl FloatOrd for f32x4 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        self.lt(x).select(self, x)
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        self.gt(x).select(self, x)
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(target_feature = "sse")]
impl FloatOrd for f32x4 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        unsafe { transmute(_mm_min_ps(transmute(self), transmute(x))) }
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        unsafe { transmute(_mm_max_ps(transmute(self), transmute(x))) }
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(not(target_feature = "avx"))]
impl FloatOrd for f32x8 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        self.lt(x).select(self, x)
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        self.gt(x).select(self, x)
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(target_feature = "avx")]
impl FloatOrd for f32x8 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        unsafe { transmute(_mm256_min_ps(transmute(self), transmute(x))) }
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        unsafe { transmute(_mm256_max_ps(transmute(self), transmute(x))) }
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(not(target_feature = "sse2"))]
impl FloatOrd for f64x2 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        self.lt(x).select(self, x)
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        self.gt(x).select(self, x)
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(target_feature = "sse2")]
impl FloatOrd for f64x2 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        unsafe { transmute(_mm_min_pd(transmute(self), transmute(x))) }
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        unsafe { transmute(_mm_max_pd(transmute(self), transmute(x))) }
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(not(target_feature = "avx"))]
impl FloatOrd for f64x4 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        self.lt(x).select(self, x)
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        self.gt(x).select(self, x)
    }
}

#[cfg(feature = "packed_simd")]
#[cfg(target_feature = "avx")]
impl FloatOrd for f64x4 {
    #[inline]
    fn fmin(self, x: Self) -> Self {
        unsafe { transmute(_mm256_min_pd(transmute(self), transmute(x))) }
    }

    #[inline]
    fn fmax(self, x: Self) -> Self {
        unsafe { transmute(_mm256_max_pd(transmute(self), transmute(x))) }
    }
}

/// A set of `FloatOrd` values.
pub trait FloatOrdSet {
    type Item;

    /// Compute the minimum value of the set. Panics if the set is empty.
    fn fmin(&self) -> Self::Item;

    /// Compute the maximum value of the set. Panics if the set is empty.
    fn fmax(&self) -> Self::Item;
}

impl<T: FloatOrd + Copy> FloatOrdSet for [T] {
    type Item = T;

    #[inline]
    fn fmin(&self) -> Self::Item {
        let mut output = self[0];
        for &x in &self[1..] {
            output = output.fmin(x);
        }
        output
    }

    #[inline]
    fn fmax(&self) -> Self::Item {
        let mut output = self[0];
        for &x in &self[1..] {
            output = output.fmax(x);
        }
        output
    }
}

impl<T: FloatOrd + Copy> FloatOrdSet for [T; 4] {
    type Item = T;

    #[inline]
    fn fmin(&self) -> Self::Item {
        [[self[0], self[1]].fmin(), [self[2], self[3]].fmin()].fmin()
    }

    #[inline]
    fn fmax(&self) -> Self::Item {
        [[self[0], self[1]].fmax(), [self[2], self[3]].fmax()].fmax()
    }
}
