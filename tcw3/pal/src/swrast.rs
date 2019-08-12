//! Software-based compositor.
//!
//! # Restrictions
//!
//!  - The maximum render target size is 16384×16384.
//!  - The coordinates of all elements (including those clipped) must fit in
//!    range ±16384.
//!  - The only supported pixel format is ARGB8888.
//!  - There's some limit on the nesting level of layers.
//!

/// `log2(TILE)`
const TILE_SHIFT: u32 = 4;
/// The tile size.
const TILE: usize = 1 << TILE_SHIFT;

/// `log2(CLIP_SUB)`
const CLIP_SUB_SHIFT: u32 = 8;
/// See `bin::Elem::clip_dist`.
const CLIP_SUB: i32 = 1 << CLIP_SUB_SHIFT;

/// `log2(UV_SUB)`
const UV_SUB_SHIFT: u32 = 16;
/// The precision of UV coordinates.
const UV_SUB: i32 = 1 << UV_SUB_SHIFT;

/// The number of internal layers.
const NUM_LAYERS: usize = 16;

pub mod bin;
pub mod layers;
