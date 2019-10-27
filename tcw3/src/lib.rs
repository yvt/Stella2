//! # TCW3 — Cross-platform GUI toolkit
//!
//! # Details
//!
//!  - **Color management**: Color values are specified in the sRGB color space,
//!    unless otherwise specified.
//!
#![feature(weak_counts)]
#![feature(doc_cfg)] // `cfg(rustdoc)`
#![allow(clippy::float_cmp)]
// this lint is ridiculous
// The size on memory hardly relates to how they are passed via a parameter
#![allow(clippy::trivially_copy_pass_by_ref)]

pub use tcw3_images as images;
pub use tcw3_pal as pal;
pub use tcw3_testing as testing;

pub mod ui;
pub mod uicore;
