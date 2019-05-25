use packed_simd::f32x4;
use std::{
    mem::transmute,
    ops::{Add, Mul},
};

#[cfg(target_feature = "sse3")]
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_feature = "sse3")]
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

pub trait SimdExt: Copy + Mul<Output = Self> {
    type Element: Copy + Add<Output = Self::Element>;

    fn splat(e: Self::Element) -> Self;

    fn extract(self, index: usize) -> Self::Element;

    /// Horizontal sum of the first two vector elements.
    ///
    /// # Examples
    ///
    ///     # use packed_simd::f32x4;
    ///     use alt_fp::SimdExt;
    ///
    ///     assert_eq!(f32x4::new(1.0, 2.0, 4.0, 8.0).sum2(), 1.0 + 2.0);
    ///
    #[inline]
    fn sum2(self) -> Self::Element {
        self.extract(0) + self.extract(1)
    }

    /// Perform a dot product using the first two vector elements and distribute
    /// the result to all lanes.
    ///
    /// # Examples
    ///
    ///     # use packed_simd::f32x4;
    ///     use alt_fp::SimdExt;
    ///
    ///     assert_eq!(
    ///         f32x4::new(1.0, 2.0, 4.0, 8.0)
    ///             .dot2_splat(f32x4::new(3.0, 7.0, 1.0, 2.0)),
    ///         f32x4::splat(1.0 * 3.0 + 2.0 * 7.0),
    ///     );
    ///
    #[inline]
    fn dot2_splat(self, other: Self) -> Self {
        Self::splat((self * other).sum2())
    }
}

impl SimdExt for f32x4 {
    type Element = f32;

    #[inline]
    fn splat(e: Self::Element) -> Self {
        f32x4::splat(e)
    }

    #[inline]
    fn extract(self, index: usize) -> Self::Element {
        self.extract(index)
    }

    #[cfg(target_feature = "sse3")]
    #[inline]
    fn sum2(self) -> Self::Element {
        let r: f32x4 = unsafe { transmute(_mm_hadd_ps(transmute(self), transmute(self))) };
        r.extract(0)
    }

    #[cfg(target_feature = "sse4.1")]
    #[inline]
    fn dot2_splat(self, other: Self) -> Self {
        unsafe { transmute(_mm_dp_ps(transmute(self), transmute(other), 0b0011_1111)) }
    }
}
