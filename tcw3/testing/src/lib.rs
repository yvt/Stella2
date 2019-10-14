//! Utilities for TCW3's testing backend.
//!
//! # `#[use_testing_wm]`
//!
//! This procedural macro, when applied to a function, wraps it with a
//! wrapper function that calls [`run_test`] to execute the contained
//! code block using TCW3's testing backend.
//!
//! [`run_test`]: tcw3_pal::testing::run_test
//!
//!     use tcw3_testing::{use_testing_wm, prelude::*};
//!     use tcw3_pal::prelude::*;
//!
//!     #[use_testing_wm(testing = "tcw3_testing")]
//!     fn test(twm: &dyn TestingWm, (num1, num2): (u32, u32)) {
//!         assert_eq!(num1, 42);
//!         assert_eq!(num2, 43);
//!         let _hwnd = twm.wm().new_wnd(Default::default());
//!     }
//!     // The macro transforms the function into:
//!     //  fn test(__arg1: (u32, u32)) {
//!     //      fn test(twm: &dyn TestingWm, (num1, num2): (u32, u32)) {
//!     //      }
//!     //      tcw3_testing::pal_testing::run_test( ... )
//!     //  }
//!
//!     test((42, 43));
//!
//! The optional argument `testing` specifies a path to this crate, defaulting
//! to `"tcw3::testing"` when not specified.
//!
//! The attribute can be combined with, for example, `#[test]` or
//! `#[quickcheck]`:
//!
//!     # use tcw3_testing::{use_testing_wm, prelude::*};
//!     # use tcw3_pal::prelude::*;
//!     #[use_testing_wm(testing = "tcw3_testing")]
//!     #[test]
//!     fn new_wnd(twm: &dyn TestingWm) {
//!         let _hwnd = twm.wm().new_wnd(Default::default());
//!     }
//!
pub use tcw3_pal::testing as pal_testing;
pub use tcw3_testing_macros::use_testing_wm;

pub mod prelude {
    pub use crate::pal_testing::TestingWm;
}
