//! Provides `CanvasImg`, an `Img` implementation based on custom drawing code.
use alt_fp::FloatOrd;
use cggeom::prelude::*;
use cgmath::{Matrix3, Vector2};
use tcw3_pal::{self as pal, prelude::*};

use super::{Bmp, HImg, Img};

/// `Img` based on custom drawing code provided via `T: `[`Paint`].
#[derive(Debug, Clone, Copy)]
struct CanvasImg<T: ?Sized> {
    paint: T,
}

#[derive(Debug)]
pub struct PaintContext<'a> {
    /// A `BitmapBuilder` object implementing [`Canvas`], with which the client
    /// should paint the image contents to a backing store.
    ///
    /// [`Canvas`]: tcw3_pal::iface::Canvas
    ///
    /// When a paint function is called, `canvas` is configured to use the
    /// the coordinate space where coordinates are represented by logical pixels
    /// and are independent of physical pixel density.
    pub canvas: &'a mut pal::BitmapBuilder,

    /// The size of the backing store measured in points (virtual pixels).
    pub size: Vector2<f32>,

    /// The target DPI scaling ratio.
    pub dpi_scale: f32,

    /// The actual DPI scaling ratio, which might differ from `dpi_scale` due to
    /// rounding of the backing store size.
    pub actual_dpi_scale: Vector2<f32>,
}

/// Represents an object that can paint the contents of [`CanvasImg`].
pub trait Paint: Send + Sync + 'static {
    /// The size of the image, measured in points.
    fn size(&self) -> Vector2<f32>;

    /// Paint the contents of the image.
    fn paint(&self, ctx: &mut PaintContext<'_>);
}

impl<T> CanvasImg<T> {
    /// Construct a `CanvasImg`.
    fn new(paint: T) -> Self {
        Self { paint }
    }
}

impl<T> Paint for (Vector2<f32>, T)
where
    T: Fn(&mut PaintContext<'_>) + Send + Sync + 'static,
{
    fn size(&self) -> Vector2<f32> {
        self.0
    }

    fn paint(&self, ctx: &mut PaintContext<'_>) {
        (self.1)(ctx);
    }
}

/// Construct a [`HImg`] from a [`Paint`].
pub fn himg_from_paint(paint: impl Paint) -> HImg {
    HImg::new(CanvasImg::new(paint))
}

/// Construct a [`HImg`] from a paint function.
pub fn himg_from_paint_fn(
    size: Vector2<f32>,
    paint: impl Fn(&mut PaintContext<'_>) + Send + Sync + 'static,
) -> HImg {
    HImg::new(CanvasImg::new((size, paint)))
}

impl<T> Img for CanvasImg<T>
where
    T: Paint,
{
    fn new_bmp(&self, dpi_scale: f32) -> Bmp {
        (self as &CanvasImg<dyn Paint>).new_bmp_dyn(dpi_scale)
    }
}

impl CanvasImg<dyn Paint> {
    fn new_bmp_dyn(&self, dpi_scale: f32) -> Bmp {
        let size = self.paint.size();

        // Compute the backing store size
        const MAX_SIZE: f32 = 16383.0;
        let backing_store_size = size * dpi_scale;
        let backing_store_size = [
            // Non-finite values are removed by these `fmax` and `fmin`
            backing_store_size.x.ceil().fmax(1.0).fmin(MAX_SIZE) as u32,
            backing_store_size.y.ceil().fmax(1.0).fmin(MAX_SIZE) as u32,
        ];

        // Calculate the actual DPI scale using the rounded backing store size
        let actual_dpi_scale = [
            backing_store_size[0] as f32 / size.x,
            backing_store_size[1] as f32 / size.y,
        ];

        // Create a bitmap
        let mut bmp_builder = pal::BitmapBuilder::new(backing_store_size);

        // Apply DPI scaling on the bitmap's drawing context
        bmp_builder.mult_transform(Matrix3::from_nonuniform_scale_2d(
            actual_dpi_scale[0],
            actual_dpi_scale[1],
        ));

        // Paint the bitmap
        self.paint.paint(&mut PaintContext {
            canvas: &mut bmp_builder,
            size,
            dpi_scale,
            actual_dpi_scale: actual_dpi_scale.into(),
        });

        // Construct a `Bmp` and return it
        (
            bmp_builder.into_bitmap(),
            (actual_dpi_scale[0] + actual_dpi_scale[1]) * 0.5,
        )
    }
}
