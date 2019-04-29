use super::super::RGBAF32;
use cggeom::{prelude::*, Box2};
use cgmath::{prelude::*, Matrix3, Matrix4, Point2, Vector2};
use cocoa::quartzcore::CATransform3D;
use core_graphics::{
    color::CGColor,
    geometry::{CGAffineTransform, CGPoint, CGRect, CGSize},
};

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
    unsafe { std::mem::transmute(m.transpose()) }
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

pub fn cg_color_from_rgbaf32(x: RGBAF32) -> CGColor {
    // TODO: Use the sRGB color space, not the generic device RGB one
    CGColor::rgb(x.r as f64, x.g as f64, x.b as f64, x.a as f64)
}

/// Convert a `Matrix3` into `CGAffineTransform`, ignoring a projective component.
pub fn cg_affine_transform_from_matrix3(mut m: Matrix3<f64>) -> CGAffineTransform {
    if m.z.z != 1.0 {
        m /= m.z.z;
    }
    CGAffineTransform::new(m.x.x, m.x.y, m.y.x, m.y.y, m.z.x, m.z.y)
}
