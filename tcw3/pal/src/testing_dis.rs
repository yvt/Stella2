//! The testing backend (disabled).
//!
//! Add a feature flag `testing` to enable the testing backend.

/// Call `with_testing_wm` if the testing backend is enabled. Otherwise,
/// output a warning message and return without calling the givne function.
///
/// This function is available even if the `testing` feature flag is disabled.
pub fn run_test(_cb: impl FnOnce(crate::Wm) + Send) {
    eprintln!("warning: testing backend is disabled, skipping some tests");
}
