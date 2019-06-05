use super::{Bmp, HImg, Img};
use crate::pal::Bitmap;

/// [`Img`] that provides a specified `Bitmap`.
#[derive(Debug, Clone)]
pub struct BitmapImg {
    bitmap: Bitmap,
    dpi_scale: f32,
}

impl BitmapImg {
    /// Construct a `BitmapImg`.
    pub fn new(bitmap: Bitmap, dpi_scale: f32) -> Self {
        Self { bitmap, dpi_scale }
    }

    /// Convert `self` to a `HImg`.
    pub fn into_hbmp(self) -> HImg {
        HImg::new(self)
    }
}

impl Img for BitmapImg {
    fn new_bmp(&self, _dpi_scale: f32) -> Bmp {
        (self.bitmap.clone(), self.dpi_scale)
    }
}
