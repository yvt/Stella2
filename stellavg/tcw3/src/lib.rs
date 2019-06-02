//! The TCW3 binding for StellaVG
use stellavg_io::{Cmd, CmdDecoder};
use tcw3::pal::{iface::Canvas, RGBAF32};

/// An extension trait for `Canvas` that provides methods for drawing
/// StellaVG images.
pub trait CanvasStellavgExt: Canvas {
    /// Draw a StellaVG image.
    fn draw_stellavg(&mut self, bytes: &[u8], options: &Options<'_>);
}

/// Options for [`CanvasStellavgExt::draw_stellavg`].
#[derive(Clone, Copy)]
pub struct Options<'a> {
    color_xform: &'a dyn Fn(RGBAF32) -> RGBAF32,
}

impl<'a> Options<'a> {
    /// Construct a `Options`.
    pub fn new() -> Self {
        Self {
            color_xform: &|x| x,
        }
    }

    /// Set the color transformation function.
    pub fn with_color_xfrom(self, value: &'a dyn Fn(RGBAF32) -> RGBAF32) -> Self {
        Self { color_xform: value }
    }
}

impl<'a> Default for Options<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Canvas + ?Sized> CanvasStellavgExt for T {
    fn draw_stellavg(&mut self, bytes: &[u8], options: &Options<'_>) {
        self.save();
        for cmd in CmdDecoder::from_bytes(bytes) {
            match cmd {
                Cmd::BeginPath => self.begin_path(),
                Cmd::Fill => self.fill(),
                Cmd::MoveTo(p) => self.move_to(p.cast().unwrap()),
                Cmd::LineTo(p) => self.line_to(p.cast().unwrap()),
                Cmd::QuadBezierTo(cps) => {
                    self.quad_bezier_to(cps[0].cast().unwrap(), cps[1].cast().unwrap())
                }
                Cmd::CubicBezierTo(cps) => self.cubic_bezier_to(
                    cps[0].cast().unwrap(),
                    cps[1].cast().unwrap(),
                    cps[2].cast().unwrap(),
                ),
                Cmd::SetFillRgb(color) => self.set_fill_rgb((options.color_xform)(RGBAF32::new(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                ))),
            }
        }
        self.restore();
    }
}
