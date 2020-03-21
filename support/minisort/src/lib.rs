//! Provides a small sort implementation.
#![cfg_attr(test, feature(is_sorted))]
#![feature(specialization)]

mod auto;
mod cstdlib;
mod insertion;
pub use self::auto::*;
pub use self::cstdlib::*;
pub use self::insertion::*;
