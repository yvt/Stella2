use winapi::um::errhandlingapi::GetLastError;

/// Panic with an error code returned by `GetLastError` if the
/// given value is equal to `T::default()` (e.g., `FALSE`, `false`).
pub fn assert_win32_ok<T: Default + PartialEq<T> + Copy>(b: T) {
    if b == T::default() {
        panic_last_error();
    }
}

/// Panic with an error code returned by `GetLastError`.
#[cold]
fn panic_last_error() -> ! {
    panic!("Win32 error 0x{:08x}", unsafe { GetLastError() });
}
