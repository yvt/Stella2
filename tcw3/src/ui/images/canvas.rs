//! Provides `CanvasImg`, an `Img` implementation based on custom drawing code.
use alt_fp::FloatOrd;
use cggeom::prelude::*;
use cgmath::{Matrix3, Vector2};

use super::{Bmp, HImg, Img};
use crate::{pal, pal::prelude::*};

/// `Img` based on custom drawing code provided via `T: `[`Paint`].
#[derive(Debug, Clone, Copy)]
struct CanvasImg<T: ?Sized> {
    paint: T,
}

pub use crate::ui::mixins::canvas::PaintContext;

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
        });

        // Construct a `Bmp` and return it
        (
            bmp_builder.into_bitmap(),
            (actual_dpi_scale[0] + actual_dpi_scale[1]) * 0.5,
        )
    }
}
