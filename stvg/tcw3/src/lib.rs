//! The TCW3 binding for StellaVG
use cggeom::prelude::*;
use cgmath::Matrix3;
use stvg_io::{Cmd, CmdDecoder};
use tcw3_images::{himg_from_paint_fn, HImg};
use tcw3_pal::{iface::Canvas, RGBAF32};

/// An extension trait for `Canvas` that provides methods for drawing
/// StellaVG images.
pub trait CanvasStvgExt: Canvas {
    /// Draw a StellaVG image.
    fn draw_stellavg(&mut self, bytes: &[u8], options: &Options<'_>);
}

/// Options for [`CanvasStvgExt::draw_stellavg`].
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
    pub fn with_color_xform(self, value: &'a dyn Fn(RGBAF32) -> RGBAF32) -> Self {
        Self { color_xform: value }
    }
}

impl<'a> Default for Options<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Canvas + ?Sized> CanvasStvgExt for T {
    fn draw_stellavg(&mut self, bytes: &[u8], options: &Options<'_>) {
        self.save();
        self.mult_transform(Matrix3::from_scale_2d(
            1.0 / (1 << stvg_io::FRAC_BITS) as f32,
        ));
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

/// The builder of `HImg` for StellaVG images.
#[derive(Debug, Clone, Copy)]
pub struct StvgImg<TBytes, TColorXform> {
    bytes: TBytes,
    size: [f32; 2],
    scale: f32,
    color_xform: TColorXform,
}

impl<TBytes> StvgImg<TBytes, fn(RGBAF32) -> RGBAF32> {
    /// Construct a `StvgImg` from StellaVG-encoded data and the size.
    pub fn new(data: (TBytes, [f32; 2])) -> Self {
        Self {
            bytes: data.0,
            size: data.1,
            scale: 1.0,
            color_xform: |x| x,
        }
    }
}

impl<TBytes, TColorXform> StvgImg<TBytes, TColorXform> {
    /// Assign `scale`, returning a new `StvgImg`.
    pub fn with_scale(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// Assign `color_xform`, returning a new `StvgImg`.
    pub fn with_color_xform<T>(self, color_xform: T) -> StvgImg<TBytes, T> {
        StvgImg {
            bytes: self.bytes,
            size: self.size,
            scale: self.scale,
            color_xform,
        }
    }
}

impl<TBytes, TColorXform> StvgImg<TBytes, TColorXform>
where
    TBytes: std::borrow::Borrow<[u8]> + Send + Sync + 'static,
    TColorXform: Fn(RGBAF32) -> RGBAF32 + Send + Sync + 'static,
{
    /// Construct a `HImg` from `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// static STVG_IMAGE: (&[u8], [f32; 2]) =
    ///     stvg_macro::include_stvg!("../tests/tiger.svgz");
    ///
    /// use stvg_tcw3::StvgImg;
    ///
    /// # #[tcw3::testing::use_testing_wm]
    /// # fn inner(twm: &dyn tcw3::pal::testing::TestingWm) {
    /// let himg = StvgImg::new(STVG_IMAGE).into_himg();
    /// # }
    /// # inner();
    /// ```
    pub fn into_himg(self) -> HImg {
        himg_from_paint_fn(
            [self.size[0] * self.scale, self.size[1] * self.scale].into(),
            move |draw_ctx| {
                let bytes = self.bytes.borrow();
                let color_xform = &self.color_xform;

                let c = &mut draw_ctx.canvas;
                c.mult_transform(Matrix3::from_scale_2d(self.scale));
                c.draw_stellavg(bytes, &Options::new().with_color_xform(color_xform));
            },
        )
    }
}

/// Create a `Fn(RGBAF32) -> RGBAF32` that replaces the color with `new_color`,
/// only preserving the original alpha value.
///
/// `new_color`'s alpha value used to modulate the input alpha value. Specify
/// `1.0` to only modify the color components.
///
/// This function is useful for recoloring StellaVG artwork via
/// [`Options::with_color_xform`] or [`StvgImg::with_color_xform`].
///
/// # Examples
///
/// ```
/// static STVG_IMAGE: (&[u8], [f32; 2]) =
///     stvg_macro::include_stvg!("../tests/tiger.svgz");
///
/// use stvg_tcw3::{StvgImg, replace_color};
///
/// # #[tcw3::testing::use_testing_wm]
/// # fn inner(twm: &dyn tcw3::pal::testing::TestingWm) {
/// let himg = StvgImg::new(STVG_IMAGE)
///     .with_color_xform(replace_color([0.4, 0.5, 0.6, 1.0]))
///     .into_himg();
/// # }
/// # inner();
/// ```
pub fn replace_color(new_color: impl Into<RGBAF32>) -> impl Fn(RGBAF32) -> RGBAF32 {
    let orig = new_color.into();
    move |color| RGBAF32::new(orig.r, orig.g, orig.b, color.a * orig.a)
}
