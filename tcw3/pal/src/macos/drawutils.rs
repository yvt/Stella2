use super::super::RGBAF32;
use cggeom::{prelude::*, Box2};
use cgmath::{Matrix3, Matrix4, Point2, Vector2};
use cocoa::{foundation::NSPoint, quartzcore::CATransform3D};
use core_foundation::base::TCFType;
use core_graphics::{
    base::CGFloat,
    color::{CGColor, SysCGColorRef},
    color_space::{kCGColorSpaceSRGB, CGColorSpace, CGColorSpaceRef},
    geometry::{CGAffineTransform, CGPoint, CGRect, CGSize},
};
use lazy_static::lazy_static;

pub fn extend_matrix3_with_identity_z(m: Matrix3<f32>) -> Matrix4<f32> {
    // ┌                 ┐
    // │ m00 m01  0  m02 │
    // │ m10 m11  0  m12 │
    // │  0   0   1   0  │
    // │ m20 m21  0   1  │
    // └                 ┘
    Matrix4::new(
        m.x.x, m.x.y, 0.0, m.x.z, m.y.x, m.y.y, 0.0, m.y.z, 0.0, 0.0, 1.0, 0.0, m.z.x, m.z.y, 0.0,
        m.z.z,
    )
}

pub fn ca_transform_3d_from_matrix4(m: Matrix4<f64>) -> CATransform3D {
    unsafe { std::mem::transmute(m) }
}

pub fn cg_rect_from_box2(bx: Box2<f64>) -> CGRect {
    CGRect::new(&cg_point_from_point2(bx.min), &cg_size_from_vec2(bx.size()))
}

pub fn cg_point_from_point2(p: Point2<f64>) -> CGPoint {
    CGPoint::new(p.x, p.y)
}

pub fn cg_size_from_vec2(p: Vector2<f64>) -> CGSize {
    CGSize::new(p.x, p.y)
}

#[allow(dead_code)]
pub fn point2_from_ns_point(p: NSPoint) -> Point2<f64> {
    Point2::new(p.x, p.y)
}

struct CGColorSpaceCell(CGColorSpace);

unsafe impl Send for CGColorSpaceCell {}
unsafe impl Sync for CGColorSpaceCell {}

lazy_static! {
    static ref CG_COLOR_SPACE_SRGB: CGColorSpaceCell =
        CGColorSpaceCell(CGColorSpace::create_with_name(unsafe { kCGColorSpaceSRGB }).unwrap());
}

/// Get the sRGB color space.
pub fn cg_color_space_srgb() -> &'static CGColorSpace {
    &CG_COLOR_SPACE_SRGB.0
}

pub fn cg_color_from_rgbaf32(x: RGBAF32) -> CGColor {
    unsafe {
        let ptr = CGColorCreate(
            (&**cg_color_space_srgb()) as *const CGColorSpaceRef as *const u8,
            [x.r as f64, x.g as f64, x.b as f64, x.a as f64].as_ptr(),
        );
        CGColor::wrap_under_create_rule(ptr)
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorCreate(space: *const u8, components: *const CGFloat) -> SysCGColorRef;
}

/// Convert a `Matrix3` into `CGAffineTransform`, ignoring a projective component.
pub fn cg_affine_transform_from_matrix3(mut m: Matrix3<f64>) -> CGAffineTransform {
    if m.z.z != 1.0 {
        m /= m.z.z;
    }
    CGAffineTransform::new(m.x.x, m.x.y, m.y.x, m.y.y, m.z.x, m.z.y)
}
