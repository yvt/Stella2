//! Provides functions for measuring strings by the number of Unicode scalar
//! values.
use packed_simd::{i8x32, m8x32, u8x32, FromCast};

const HAS_U8X16: bool = cfg!(all(
    any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "sse2"
)) || cfg!(all(target_aarch = "aarch64", target_feature = "neon"))
    || cfg!(all(
        target_aarch = "arm",
        target_feature = "v7",
        target_feature = "neon"
    ));

fn is_utf8_continuation(x: u8) -> bool {
    (x as i8) < -0x40
}

fn is_not_utf8_continuation_u8x32(x: u8x32) -> m8x32 {
    i8x32::from_cast(x).ge(i8x32::splat(-0x40))
}

/// Find the byte offset of the first scalar value after `i` in a given UTF-8
/// string. Returns `s.len()` if there is no such scalar value.
///
/// `i` must be on a scalar boundary.
///
/// # Example
///
///     use unicount::str_next;
///     assert_eq!(str_next("искра", 0), 2);
///     assert_eq!(str_next("искра", 2), 4);
///     assert_eq!(str_next("искра", 10), 10);
///
pub fn str_next(s: &str, i: usize) -> usize {
    utf8_str_next(s.as_bytes(), i)
}

/// Find the byte offset of the first scalar value before `i` in a given UTF-8
/// string. Returns `0` if there is no such scalar value.
///
/// `i` must be on a scalar boundary.
///
/// # Example
///
///     use unicount::str_prev;
///     assert_eq!(str_prev("искра", 10), 8);
///     assert_eq!(str_prev("искра", 8), 6);
///     assert_eq!(str_prev("искра", 0), 0);
///
pub fn str_prev(s: &str, i: usize) -> usize {
    utf8_str_prev(s.as_bytes(), i)
}

/// Find the byte offset of the first scalar boundary at or after `i` in a
/// given UTF-8 string.
///
/// `i` doesn't need to be on a scalar boundary. `i` must be in range
/// `0..=s.len()`.
///
/// # Example
///
///     use unicount::str_ceil;
///     assert_eq!(str_ceil("искра", 6), 6);
///     assert_eq!(str_ceil("искра", 7), 8);
///     assert_eq!(str_ceil("искра", 8), 8);
///
pub fn str_ceil(s: &str, i: usize) -> usize {
    utf8_str_ceil(s.as_bytes(), i)
}

/// Find the byte offset of the first scalar boundary at or before `i` in a
/// given UTF-8 string.
///
/// `i` doesn't need to be on a scalar boundary. `i` must be in range
/// `0..=s.len()`.
///
/// # Example
///
///     use unicount::str_floor;
///     assert_eq!(str_floor("искра", 6), 6);
///     assert_eq!(str_floor("искра", 7), 6);
///     assert_eq!(str_floor("искра", 8), 8);
///
pub fn str_floor(s: &str, i: usize) -> usize {
    utf8_str_floor(s.as_bytes(), i)
}

/// Mapping from the hi-nibbles of UTF-8-encoded bytes to continuation byte
/// counts.
//
//                                          ┌ 1110 (U+0800–U+FFFF)
//                                          │     ┌ 10?? (continuation bytes)
//                                          ╧═    ╧═══════
const NIBBLE_TO_CONTINUATION_LEN: u32 = 0b11100101000000000000000000000000;
//                                        ╤═  ╤═══        ╤═══════════════
//                                        │   │           └ 0???? (U+0000–U+007F)
//                                        │   └ 110? (U+0080–U+07FF)
//                                        └ 1111 (U+10000–U+10FFFF)

const fn nibble_to_continuation_len(x: u8) -> u8 {
    ((NIBBLE_TO_CONTINUATION_LEN >> (x * 2)) & 0b11) as u8
}

/// Find the byte offset of the first scalar value after `i` in a given byte
/// slice assumed to be a UTF-8 string. Returns `s.len()` if there is no such
/// scalar value.
///
/// `i` must be on a scalar boundary.
pub fn utf8_str_next(s: &[u8], mut i: usize) -> usize {
    debug_assert!(i <= s.len());
    if i < s.len() {
        // `i` must be on a scalar boundary
        debug_assert!(
            !is_utf8_continuation(s[i]),
            "`i` is not on a scalar boundary"
        );

        i += nibble_to_continuation_len(s[i] >> 4) as usize + 1;

        debug_assert!(
            i <= s.len(),
            "cursor went past the end - `s` may be malformed"
        );
    }
    i
}

/// Find the byte offset of the first scalar value before `i` in a given byte
/// slice assumed to be a UTF-8 string. Returns `0` if there is no such
/// scalar value.
///
/// `i` must be on a scalar boundary.
pub fn utf8_str_prev(s: &[u8], mut i: usize) -> usize {
    debug_assert!(i <= s.len());

    // `i` must be on a scalar boundary
    debug_assert!(i >= s.len() || !is_utf8_continuation(s[i]));

    if i > 0 {
        while {
            i -= 1;
            i > 0 && is_utf8_continuation(s[i])
        } {}
    }
    i
}

/// Find the byte offset of the first scalar boundary at or after `i` in a
/// given byte slice assumed to be a UTF-8 string.
///
/// `i` doesn't need to be on a scalar boundary. `i` must be in range
/// `0..=s.len()`.
pub fn utf8_str_ceil(s: &[u8], mut i: usize) -> usize {
    debug_assert!(i <= s.len());

    while i < s.len() && is_utf8_continuation(s[i]) {
        i += 1;
    }

    i
}

/// Find the byte offset of the first scalar boundary at or before `i` in a
/// given byte slice assumed to be a UTF-8 string.
///
/// `i` doesn't need to be on a scalar boundary. `i` must be in range
/// `0..=s.len()`.
pub fn utf8_str_floor(s: &[u8], mut i: usize) -> usize {
    debug_assert!(i <= s.len());

    if i < s.len() {
        for &b in s[0..=i].iter().rev() {
            if !is_utf8_continuation(b) {
                break;
            }
            i -= 1;
        }
    }

    i
}

/// Calculate the number of scalar values in a given UTF-8 string.
///
/// # Example
///
///     use unicount::num_scalars_in_str;
///     assert_eq!(num_scalars_in_str("искра"), "искра".chars().count());
///
pub fn num_scalars_in_str(s: &str) -> usize {
    num_scalars_in_utf8_str(s.as_bytes())
}

/// Calculate the number of scalar values in a given byte slice assumed to be a
/// UTF-8 string.
pub fn num_scalars_in_utf8_str(mut s: &[u8]) -> usize {
    let mut count = 0usize;

    // Native `u8x16` support is enough for emulating `u8x32`
    if HAS_U8X16 {
        // Count the non-continuation bytes
        while s.len() >= 32 {
            let s32 = u8x32::from_slice_unaligned(&s[0..32]);
            count += is_not_utf8_continuation_u8x32(s32).bitmask().count_ones() as usize;
            s = &s[32..];
        }
    }

    // Count the non-continuation bytes
    //
    // Actually, LLVM can auto-vectorize this loop, though the compiled code
    // is large and doesn't look pretty.
    count += s.iter().filter(|&&i| !is_utf8_continuation(i)).count();

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;
    use std::convert::TryFrom;

    fn mk_random_str(v: &[u8]) -> String {
        v.chunks_exact(4)
            .map(|c| (u32::from_le_bytes([c[0], c[1], c[2], c[3]]) % 0x20000) >> (c[3] % 16))
            .filter_map(|c| char::try_from(c).ok())
            .collect()
    }

    #[quickcheck]
    fn test_num_scalars_in_str(encoded: Vec<u8>) -> bool {
        let st = mk_random_str(&encoded);
        log::debug!("st = {:?} ({:x?})", st, st.as_bytes());

        assert_eq!(num_scalars_in_str(&st), st.chars().count());
        true
    }

    #[quickcheck]
    fn test_str_next_prev(encoded: Vec<u8>) -> bool {
        let st = mk_random_str(&encoded);
        log::debug!("st = {:?}", st);

        let chars: Vec<_> = st
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(st.len()))
            .collect();

        assert_eq!(str_next(&st, st.len()), st.len());
        for w in chars.windows(2) {
            assert_eq!(str_next(&st, w[0]), w[1]);
        }

        assert_eq!(str_prev(&st, 0), 0);
        for w in chars.windows(2) {
            assert_eq!(str_prev(&st, w[1]), w[0]);
        }

        true
    }

    #[quickcheck]
    fn test_str_floor_ceil(encoded: Vec<u8>) -> bool {
        let st = mk_random_str(&encoded);
        log::debug!("st = {:?}", st);

        let chars: Vec<_> = st
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(st.len()))
            .collect();

        for &i in chars.iter() {
            assert_eq!(str_floor(&st, i), i);
            assert_eq!(str_ceil(&st, i), i);
        }
        for w in chars.windows(2) {
            for i in w[0] + 1..w[1] {
                assert_eq!(str_floor(&st, i), w[0]);
                assert_eq!(str_ceil(&st, i), w[1]);
            }
        }

        true
    }
}
