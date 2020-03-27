//! Manages DPI-independent images. Provides an application-global image
//! manager that automatically rasterizes and caches images for requested
//! DPI scale values.
//!
//! This crate is reexported by TCW3 as `tcw3::images`.
mod bitmap;
mod canvas;
mod figures;
mod img;
pub use self::{bitmap::*, canvas::*, figures::*, img::*};

// Re-exports for macros defined in this crate
#[doc(hidden)]
pub use tcw3_pal::RGBAF32;
