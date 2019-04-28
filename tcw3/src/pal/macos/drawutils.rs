use super::super::RGBAF32;
use cggeom::{prelude::*, Box2};
use cgmath::{prelude::*, Matrix4, Point2, Vector2};
use cocoa::quartzcore::CATransform3D;
use core_graphics::{geometry::{CGPoint, CGRect, CGSize}, color::CGColor};

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
