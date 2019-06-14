//! # TCW3 — Cross-platform GUI toolkit
//!
//! # Details
//!
//!  - **Color management**: Color values are specified in the sRGB color space,
//!    unless otherwise specified.
//!
#![feature(weak_ptr_eq)]
#![feature(weak_counts)]
#![feature(doc_cfg)] // `cfg(rustdoc)`
#![feature(const_vec_new)] // `Vec::new()` in a constant context

pub use tcw3_pal as pal;

pub mod ui;
pub mod uicore;
