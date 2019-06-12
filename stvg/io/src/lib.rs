//! Implements the decoder and encoder of the StellaVG (Stella Vector Graphics)
//! format.
use cgmath::Point2;
use rgb::RGBA8;
use std::mem::size_of;

mod dec;
mod enc;

pub use {dec::CmdDecoder, enc::CmdEncoder};

/// The op flags indicates which operation to perform. When multiple flags are
/// specified, the corresponding operations are performed from LSB to MSB.
mod op {
    /// Set the current fill color. Followed by a 4-byte color value.
    pub const SET_FILL_RGB: u8 = 1 << SET_FILL_RGB_SHIFT;
    pub const SET_FILL_RGB_SHIFT: u32 = 0;

    /// Fill the current path.
    pub const FILL: u8 = 1 << FILL_SHIFT;
    pub const FILL_SHIFT: u32 = 1;

    /// Clear the current path.
    pub const BEGIN_PATH: u8 = 1 << BEGIN_PATH_SHIFT;
    pub const BEGIN_PATH_SHIFT: u32 = 2;

    /// Add vertices to the current path. Does not implicitly clear the current
    /// path. Followed by path data. The path data is organized in the
    /// structure-of-arrays style in hopes of efficient application of
    /// data compression on the encoded data.
    ///
    /// This flag must be the last one.
    pub const CONTOUR: u8 = 1 << CONTOUR_SHIFT;
    pub const CONTOUR_SHIFT: u32 = 3;
}

const BYTES_PER_POINT: usize = 4;
const CONTOUR_HDR_SIZE: usize = BYTES_PER_POINT + size_of::<u16>();

/// The number of fractional bits included in fixed-point numbers used by
/// StellaVG.
///
/// Coordinates are represented by fixed-point numbers of type `i16`. Since
/// four bits (= this constant) are allocated for fractional digits, the maximum
/// representable range is circa `[-2048, 2048]`.
pub const FRAC_BITS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cmd {
    BeginPath,
    Fill,
    MoveTo(Point2<i16>),
    LineTo(Point2<i16>),
    QuadBezierTo([Point2<i16>; 2]),
    CubicBezierTo([Point2<i16>; 3]),
    SetFillRgb(RGBA8),
}
