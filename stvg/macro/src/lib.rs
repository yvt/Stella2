//! Provides a macro for generating and embedding StellaVG data at compile time.
//!
//! # Examples
//!
//! ```
//! use stvg_macro::include_stvg;
//! static TIGER: (&[u8], [f32; 2]) = include_stvg!("../tests/tiger.svgz");
//! println!("len = {}", TIGER.0.len());
//! println!("size = {:?}", TIGER.1);
//! ```

/// Include the specified SVG file as StellaVG data (`([u8; _], [f32; 2])`).
///
/// The path is relative to `$CARGO_MANIFEST_DIR`.
///
/// Be aware that the range of coordinates are limited by the internal
/// representation used by StellaVG. See [`stvg_io::FRAC_BITS`].
pub use stvg_macro_impl::include_stvg;
