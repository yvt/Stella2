//! Provides functions for measuring strings by the number of UTF-16 units.
use packed_simd::u8x16;

/// Mapping from the hi-nibbles of UTF-8-encoded bytes to UTF-16 unit counts.
//
//                                   ┌ 1110 (U+0800–U+FFFF)
//                                   │     ┌ 10?? (continuation bytes)
//                                   ╧═    ╧═══════
const NIBBLE_TO_UTF16_LEN: u32 = 0b10010101000000000101010101010101;
//                                 ╤═  ╤═══        ╤═══════════════
//                                 │   │           └ 0???? (U+0000–U+007F)
//                                 │   └ 110? (U+0080–U+07FF)
//                                 └ 1111 (U+10000–U+10FFFF)

const fn nibble_to_utf16_len(x: u8) -> u8 {
    ((NIBBLE_TO_UTF16_LEN >> (x * 2)) & 0b11) as u8
}

const NIBBLE_TO_UTF16_LEN_U8X16: u8x16 = u8x16::new(
    nibble_to_utf16_len(0),
    nibble_to_utf16_len(1),
    nibble_to_utf16_len(2),
    nibble_to_utf16_len(3),
    nibble_to_utf16_len(4),
    nibble_to_utf16_len(5),
    nibble_to_utf16_len(6),
    nibble_to_utf16_len(7),
    nibble_to_utf16_len(8),
    nibble_to_utf16_len(9),
    nibble_to_utf16_len(10),
    nibble_to_utf16_len(11),
    nibble_to_utf16_len(12),
    nibble_to_utf16_len(13),
    nibble_to_utf16_len(14),
    nibble_to_utf16_len(15),
);

/// Copied from [`Shuffle1Dyn`'s `cfg` attributes]. `true` means the target
/// architecture natively supports `u8x16::shuffle1_dyn`.
///
/// [`Shuffle1Dyn`'s `cfg` attributes]: https://github.com/rust-lang/packed_simd/blob/62a32e7b773b9176f8e13b52ee425aad9c88c3f2/src/codegen/shuffle1_dyn.rs#L87-L166
const HAS_U8X16_SHUFFLE1_DYN: bool = cfg!(all(
    any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "ssse3"
)) || cfg!(all(
    target_aarch = "aarch64",
    target_feature = "neon",
    any(feature = "core_arch", libcore_neon)
)) || cfg!(all(
    target_aarch = "arm",
    target_feature = "v7",
    target_feature = "neon",
    any(feature = "core_arch", libcore_neon)
));

/// Get the number of UTF-16 units for a given UTF-8 string.
///
/// # Performance
///
/// This function is faster than `s.encode_utf16().count()` by a factor of
/// 3–15 (measured on a Skylake processor). The code size is not so different.
///
/// # Examples
///
///     use utf16count::utf16_len;
///     assert_eq!(utf16_len(""), 0);
///     assert_eq!(utf16_len("hello"), 5);
///     assert_eq!(utf16_len("рыба"), 4);
///     assert_eq!(utf16_len("👨‍👩‍👦"), 8);
///
pub fn utf16_len_of_utf8_str(mut s: &[u8]) -> usize {
    let mut count = 0;

    if HAS_U8X16_SHUFFLE1_DYN {
        while s.len() >= 64 {
            // When building with `-Copt-level=3`, the codegen automatically
            // unrolls this loop by a factor of 4, but doesn't factor out
            // `wrapping_sum`.
            // This means manually unrolling the loop is actually good for both
            // code size and throughput.
            let accum = {
                let s16 = u8x16::from_slice_unaligned(&s[0..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            } + {
                let s16 = u8x16::from_slice_unaligned(&s[16..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            } + {
                let s16 = u8x16::from_slice_unaligned(&s[32..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            } + {
                let s16 = u8x16::from_slice_unaligned(&s[48..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            };

            count += accum.wrapping_sum() as usize;

            s = &s[64..];
        }
    }

    for b in s.iter() {
        count += nibble_to_utf16_len(b >> 4) as usize;
    }

    count
}

/// Get the number of UTF-16 units for a given UTF-8 string.
///
/// See [`utf16_len_of_utf8_str`] for more.
pub fn utf16_len(s: &str) -> usize {
    utf16_len_of_utf8_str(s.as_bytes())
}

/// Result type of [`find_utf16_pos_in_utf8_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FindUtf16PosResult {
    /// The search position within the input string.
    pub utf8_cursor: usize,
    /// The provided `utf16_pos` minus the number of UTF-16 units found in the
    /// input string.
    pub utf16_extra: usize,
}

impl FindUtf16PosResult {
    /// Return `Some(i)` if the position corresponding to `utf16_pos` was found
    /// in the input string `s` and the position is `i` (in range `0..=s.len()`).
    pub fn utf8_pos(&self) -> Option<usize> {
        if self.utf16_extra == 0 {
            Some(self.utf8_cursor)
        } else {
            None
        }
    }
}

/// Convert the given UTF-16 index to a UTF-8 index.
///
/// The result can be one of the following:
///
///  - `FindUtf16PosResult { utf8_cursor: i, utf16_extra: 0 }`: The position
///    corresponding to `utf16_pos` was found in `s` and the position is
///     `i`. Use [`FindUtf16PosResult::utf8_pos`] if you are only interested
///    in handling this case.
///
///  - `FindUtf16PosResult { utf8_cursor: i, utf16_extra: 1 } if i < s.len()`:
///    `utf16_pos` falls between a surrogate pair created from the UTF-8
///    sequence at `s[i..i + 4]`.
///
///  - `FindUtf16PosResult { utf8_cursor: s.len(), utf16_extra: i } if i > 0`:
///    The corresponding position was not found in `s` because `s` contains
///    only `utf16_pos - i` UTF-16 units.
///
/// # Examples
///
///     use utf16count::{find_utf16_pos, FindUtf16PosResult};
///
///     assert_eq!(find_utf16_pos(2, "рыба"), FindUtf16PosResult {
///         utf8_cursor: 4,
///         utf16_extra: 0,
///     });
///     assert_eq!(find_utf16_pos(4, "рыба"), FindUtf16PosResult {
///         utf8_cursor: 8,
///         utf16_extra: 0,
///     });
///
///     // Out of bounds
///     assert_eq!(find_utf16_pos(7, "рыба"), FindUtf16PosResult {
///         utf8_cursor: 8,
///         utf16_extra: 3,
///     });
///
///     // 👨‍👩‍👦 starts with a surrogate pair
///     assert_eq!(find_utf16_pos(1, "👨‍👩‍👦"), FindUtf16PosResult {
///         utf8_cursor: 0,
///         utf16_extra: 1,
///     });
///
pub fn find_utf16_pos_in_utf8_str(mut utf16_pos: usize, mut s: &[u8]) -> FindUtf16PosResult {
    let mut utf8_cursor = 0;

    if HAS_U8X16_SHUFFLE1_DYN {
        while s.len() >= 64 {
            let accum = {
                let s16 = u8x16::from_slice_unaligned(&s[0..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            } + {
                let s16 = u8x16::from_slice_unaligned(&s[16..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            } + {
                let s16 = u8x16::from_slice_unaligned(&s[32..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            } + {
                let s16 = u8x16::from_slice_unaligned(&s[48..]);
                NIBBLE_TO_UTF16_LEN_U8X16.shuffle1_dyn(s16 >> 4)
            };

            let chunk_u16len = accum.wrapping_sum() as usize;

            if chunk_u16len > utf16_pos {
                break;
            }

            s = &s[64..];
            utf8_cursor += 64;
            utf16_pos -= chunk_u16len;
        }
    }

    for b in s.iter() {
        let u16len = nibble_to_utf16_len(b >> 4) as usize;

        if u16len > utf16_pos {
            break;
        }

        utf8_cursor += 1;
        utf16_pos -= u16len;
    }

    FindUtf16PosResult {
        utf8_cursor,
        utf16_extra: utf16_pos,
    }
}

/// Convert the given UTF-16 index to a UTF-8 index.
///
/// See [`find_utf16_pos_in_utf8_str`] for more.
pub fn find_utf16_pos(utf16_pos: usize, s: &str) -> FindUtf16PosResult {
    find_utf16_pos_in_utf8_str(utf16_pos, s.as_bytes())
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
    fn test_utf16_len(v: Vec<u8>) -> bool {
        let st = mk_random_str(&v);
        log::debug!("st = {:?}", st);

        let got = utf16_len(&st);
        let expected = st.encode_utf16().count();
        log::debug!("got = {:?}, expected = {:?}", got, expected);

        got == expected
    }

    #[quickcheck]
    fn test_find_utf16_pos(v: Vec<u8>, extra: usize) -> bool {
        let st = mk_random_str(&v);
        log::debug!("st = {:?}", st);

        let u16_len = utf16_len(&st);

        for i in 0..=u16_len {
            let ret = find_utf16_pos(i, &st);
            if i - ret.utf16_extra != utf16_len(&st[0..ret.utf8_cursor]) {
                return false;
            }
        }

        assert_eq!(
            find_utf16_pos(u16_len + extra, &st),
            FindUtf16PosResult {
                utf8_cursor: st.len(),
                utf16_extra: extra
            }
        );

        true
    }
}
