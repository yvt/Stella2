//! This crate provides an alternative (faster in most cases) implementation for
//! floating-point operations.
pub mod cast;
pub mod cmp;
pub mod fma;
pub mod simd;

#[doc(no_inline)]
pub use self::{cast::*, cmp::*, fma::*, simd::*};
