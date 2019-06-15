use tcw3_pal::Bitmap;

use super::{Bmp, HImg, Img};

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
    ///
    /// This method just calls `HImg::new(self)`.
    pub fn into_hbmp(self) -> HImg {
        HImg::new(self)
    }
}

impl Img for BitmapImg {
    fn new_bmp(&self, _dpi_scale: f32) -> Bmp {
        (self.bitmap.clone(), self.dpi_scale)
    }
}
