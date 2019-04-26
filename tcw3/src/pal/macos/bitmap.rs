use core_graphics::{
    color_space::{kCGColorSpaceSRGB, CGColorSpace},
    context::CGContext,
    image::{CGImage, CGImageAlphaInfo},
};

use super::super::traits;

#[derive(Clone)]
pub struct Bitmap {
    cg_image: CGImage,
}

unsafe impl Send for Bitmap {}
unsafe impl Sync for Bitmap {}

impl traits::Bitmap for Bitmap {}

pub struct BitmapBuilder {
    cg_context: CGContext,
}

impl traits::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        let cg_image = self.cg_context.create_image().unwrap();
        Bitmap { cg_image }
    }
}

impl traits::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        // Get the sRGB color space
        let cs = CGColorSpace::create_with_name(unsafe { kCGColorSpaceSRGB }).unwrap();

        Self {
            cg_context: CGContext::create_bitmap_context(
                None,         // data
                size[0] as _, // width
                size[1] as _, // width
                8,            // bits_per_component
                0,            // bytes_per_row
                &cs,
                CGImageAlphaInfo::CGImageAlphaPremultipliedLast as u32,
            ),
        }
    }
}

impl traits::Canvas for BitmapBuilder {}
