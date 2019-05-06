use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use core_graphics::{
    color_space::{kCGColorSpaceSRGB, CGColorSpace},
    context::{CGContext, CGLineCap, CGLineJoin},
    image::{CGImage, CGImageAlphaInfo},
};
use std::fmt;

use super::super::{iface, LineCap, LineJoin, RGBAF32};
use super::drawutils::{cg_affine_transform_from_matrix3, cg_rect_from_box2};

#[derive(Clone)]
pub struct Bitmap {
    pub(super) cg_image: CGImage,
}

unsafe impl Send for Bitmap {}
unsafe impl Sync for Bitmap {}

impl fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cg_image: *mut () = unsafe { std::mem::transmute_copy(&self.cg_image) };
        f.debug_struct("Bitmap")
            .field("cg_image", &cg_image)
            .finish()
    }
}

impl iface::Bitmap for Bitmap {
    fn size(&self) -> [u32; 2] {
        [self.cg_image.width() as u32, self.cg_image.height() as u32]
    }
}

pub struct BitmapBuilder {
    pub(super) cg_context: CGContext,
}

impl fmt::Debug for BitmapBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cg_context: *mut () = unsafe { std::mem::transmute_copy(&self.cg_context) };
        f.debug_struct("BitmapBuilder")
            .field("cg_context", &cg_context)
            .finish()
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        let cg_image = self.cg_context.create_image().unwrap();
        Bitmap { cg_image }
    }
}

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        // Get the sRGB color space
        let cs = CGColorSpace::create_with_name(unsafe { kCGColorSpaceSRGB }).unwrap();

        let cg_context = CGContext::create_bitmap_context(
            None,         // data
            size[0] as _, // width
            size[1] as _, // width
            8,            // bits_per_component
            0,            // bytes_per_row
            &cs,
            CGImageAlphaInfo::CGImageAlphaPremultipliedLast as u32,
        );

        // Flip vertically to match TCW3's coordinate space
        cg_context.scale(1.0, -1.0);
        cg_context.translate(0.0, -(size[1] as f64));

        Self { cg_context }
    }
}

impl iface::Canvas for BitmapBuilder {
    fn save(&mut self) {
        self.cg_context.save();
    }
    fn restore(&mut self) {
        self.cg_context.restore();
    }

    fn begin_path(&mut self) {
        self.cg_context.begin_path();
    }
    fn close_path(&mut self) {
        self.cg_context.close_path();
    }

    fn move_to(&mut self, p: Point2<f32>) {
        self.cg_context.move_to_point(p.x as f64, p.y as f64);
    }
    fn line_to(&mut self, p: Point2<f32>) {
        self.cg_context.add_line_to_point(p.x as f64, p.y as f64);
    }
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>) {
        self.cg_context.add_curve_to_point(
            cp1.x as f64,
            cp1.y as f64,
            cp2.x as f64,
            cp2.y as f64,
            p.x as f64,
            p.y as f64,
        );
    }
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>) {
        self.cg_context
            .add_quad_curve_to_point(cp.x as f64, cp.y as f64, p.x as f64, p.y as f64);
    }

    fn fill(&mut self) {
        self.cg_context.fill_path();
    }
    fn stroke(&mut self) {
        self.cg_context.stroke_path();
    }
    fn clip(&mut self) {
        self.cg_context.clip();
    }

    fn stroke_rect(&mut self, bx: Box2<f32>) {
        self.cg_context
            .stroke_rect(cg_rect_from_box2(bx.cast().unwrap()));
    }
    fn fill_rect(&mut self, bx: Box2<f32>) {
        self.cg_context
            .fill_rect(cg_rect_from_box2(bx.cast().unwrap()));
    }
    fn clip_rect(&mut self, bx: Box2<f32>) {
        self.cg_context
            .clip_to_rect(cg_rect_from_box2(bx.cast().unwrap()));
    }

    fn set_fill_rgb(&mut self, rgb: RGBAF32) {
        self.cg_context
            .set_rgb_fill_color(rgb.r as f64, rgb.g as f64, rgb.b as f64, rgb.a as f64);
    }
    fn set_stroke_rgb(&mut self, rgb: RGBAF32) {
        self.cg_context.set_rgb_stroke_color(
            rgb.r as f64,
            rgb.g as f64,
            rgb.b as f64,
            rgb.a as f64,
        );
    }

    fn set_line_cap(&mut self, cap: LineCap) {
        self.cg_context.set_line_cap(match cap {
            LineCap::Butt => CGLineCap::CGLineCapButt,
            LineCap::Round => CGLineCap::CGLineCapRound,
            LineCap::Square => CGLineCap::CGLineCapSquare,
        });
    }
    fn set_line_join(&mut self, join: LineJoin) {
        self.cg_context.set_line_join(match join {
            LineJoin::Bevel => CGLineJoin::CGLineJoinBevel,
            LineJoin::Miter => CGLineJoin::CGLineJoinMiter,
            LineJoin::Round => CGLineJoin::CGLineJoinRound,
        });
    }
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]) {
        let lengths: Vec<_> = lengths.iter().map(|x| *x as f64).collect();
        self.cg_context.set_line_dash(phase as f64, &lengths);
    }
    fn set_line_width(&mut self, width: f32) {
        self.cg_context.set_line_width(width as f64);
    }
    fn set_line_miter_limit(&mut self, miter_limit: f32) {
        self.cg_context.set_miter_limit(miter_limit as f64);
    }

    fn mult_transform(&mut self, m: Matrix3<f32>) {
        self.cg_context
            .concat_ctm(cg_affine_transform_from_matrix3(m.cast().unwrap()));
    }
}
