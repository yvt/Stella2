use std::{convert::TryInto, ptr::null_mut};
use winapi::um::{
    stringapiset::{MultiByteToWideChar, WideCharToMultiByte},
    winnls::CP_UTF8,
};

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

/// Convert a given `[WCHAR]` into `str`
///
/// Panics if the input string is too long.
pub fn wstr_to_str(s: &[u16]) -> Box<str> {
    if s.len() == 0 {
        String::new().into_boxed_str()
    } else {
        unsafe {
            let in_len = s.len().try_into().expect("string too long");
            let num_bytes = WideCharToMultiByte(
                CP_UTF8,
                0,
                s.as_ptr(),
                in_len,
                null_mut(),
                0,
                null_mut(),
                null_mut(),
            );
            assert_win32_ok(num_bytes);

            let len: usize = num_bytes.try_into().expect("string too long");

            let mut out = Vec::<u8>::with_capacity(len);
            assert_eq!(
                WideCharToMultiByte(
                    CP_UTF8,
                    0,
                    s.as_ptr(),
                    in_len,
                    out.as_mut_ptr() as *mut i8,
                    num_bytes,
                    null_mut(),
                    null_mut()
                ),
                num_bytes
            );
            out.set_len(len);

            String::from_utf8_unchecked(out).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wchar::{wch, wch_c};

    #[test]
    fn test_str_to_c_wstr() {
        assert_eq!(*str_to_c_wstr(""), *wch_c!(""));
        assert_eq!(*str_to_c_wstr("книга"), *wch_c!("книга"));
        assert_eq!(*str_to_c_wstr("🦄✨"), *wch_c!("🦄✨"));
    }

    #[test]
    fn test_wstr_to_str() {
        assert_eq!(*wstr_to_str(wch!("")), *"");
        assert_eq!(*wstr_to_str(wch!("книга")), *"книга");
        assert_eq!(*wstr_to_str(wch!("🦄✨")), *"🦄✨");
    }
}
