/// Convert a 23-bit unsigned integer to a single-precision floating-point
/// number.
///
/// If the input is out of range, the result is unspecified.
///
/// # Examples
///
///     use alt_fp::u23_to_f32;
///     assert_eq!(u23_to_f32(0), 0.0);
///     assert_eq!(u23_to_f32(1), 1.0);
///     assert_eq!(u23_to_f32(8388606), 8388606.0);
///     assert_eq!(u23_to_f32(8388607), 8388607.0);
///
#[inline]
pub fn u23_to_f32(x: u32) -> f32 {
    <f32>::from_bits(x | 0x4b000000) - 8388608.0
}

/// Convert a 16-bit unsigned integer to a single-precision floating-point
/// number.
///
/// # Examples
///
///     use alt_fp::u16_to_f32;
///     assert_eq!(u16_to_f32(0), 0.0);
///     assert_eq!(u16_to_f32(1), 1.0);
///     assert_eq!(u16_to_f32(65534), 65534.0);
///     assert_eq!(u16_to_f32(65535), 65535.0);
///
#[inline]
pub fn u16_to_f32(x: u16) -> f32 {
    u23_to_f32(x as u32)
}

/// Convert a single-precision floating-point number to a 23-bit unsigned
/// integer.
///
/// The rounding mode is based on the default [floating-point environment].
/// Do not use `<f32>::trunc` to enforce the round-toward-zero rounding mode;
/// the `as` operator would be faster on all modern x86 processors.
///
/// [floating-point environment]: http://llvm.org/docs/LangRef.html#floatenv
///
/// If the input is out of range, the result is unspecified.
///
/// # Examples
///
///     use alt_fp::f32_to_u23;
///     assert_eq!(f32_to_u23(0.0), 0);
///     assert_eq!(f32_to_u23(1.0), 1);
///     assert_eq!(f32_to_u23(1.5), 2);
///     assert_eq!(f32_to_u23(8388606.0), 8388606);
///     assert_eq!(f32_to_u23(8388607.0), 8388607);
///
#[inline]
pub fn f32_to_u23(x: f32) -> u32 {
    (x + 8388608.0).to_bits() & 0x7fffff
}
