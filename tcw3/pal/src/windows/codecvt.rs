use std::{convert::TryInto, ptr::null_mut};
use winapi::um::{stringapiset::MultiByteToWideChar, winnls::CP_UTF8};

use super::utils::assert_win32_ok;

/// Convert a given `str` into a null-terminated wide character string.
///
/// Panics if the input string is too long.
///
/// For a static string, please use `wchar::wch_c!("foo")` instead.
pub fn str_to_c_wstr(s: &str) -> Box<[u16]> {
    if s.len() == 0 {
        Box::new([0])
    } else {
        unsafe {
            let in_len = s.len().try_into().expect("string too long");
            let num_wchars =
                MultiByteToWideChar(CP_UTF8, 0, s.as_ptr() as *const i8, in_len, null_mut(), 0);
            assert_win32_ok(num_wchars);

            let len: usize = num_wchars.try_into().expect("string too long");
            let len = len.checked_add(1).expect("string too long"); // null termination

            let mut out = Vec::<u16>::with_capacity(len);
            assert_eq!(
                MultiByteToWideChar(
                    CP_UTF8,
                    0,
                    s.as_ptr() as *const i8,
                    in_len,
                    out.as_mut_ptr(),
                    num_wchars
                ),
                num_wchars
            );
            out.as_mut_ptr().offset(len as isize - 1).write(0); // null termination
            out.set_len(len);

            out.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wchar::wch_c;

    #[test]
    fn test_str_to_c_wstr() {
        assert_eq!(*str_to_c_wstr(""), *wch_c!(""));
        assert_eq!(*str_to_c_wstr("книга"), *wch_c!("книга"));
        assert_eq!(*str_to_c_wstr("🦄✨"), *wch_c!("🦄✨"));
    }
}
