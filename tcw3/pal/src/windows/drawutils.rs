use alt_fp::FloatOrd;
use cgmath::{Matrix3, Matrix4};
use winrt::{
    windows::foundation::numerics::{Matrix3x2, Matrix4x4, Vector2},
    windows::ui::Color,
};

pub trait ExtendExt<T> {
    type Output;

    fn extend(self, e: T) -> Self::Output;
}

impl<T> ExtendExt<T> for cgmath::Point2<T> {
    type Output = cgmath::Point3<T>;

    fn extend(self, e: T) -> Self::Output {
        cgmath::Point3::new(self.x, self.y, e)
    }
}

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

pub fn winrt_m4x4_from_cgmath(m: Matrix4<f32>) -> Matrix4x4 {
    unsafe { std::mem::transmute(m) }
}

/// Convert a `Matrix3` into `Matrix3x2`, ignoring a projective component.
pub fn winrt_m3x2_from_cgmath(mut m: Matrix3<f32>) -> Matrix3x2 {
    if m.z.z != 1.0 {
        m /= m.z.z;
    }
    Matrix3x2 {
        M11: m.x.x,
        M12: m.x.y,
        M21: m.y.x,
        M22: m.y.y,
        M31: m.z.x,
        M32: m.z.y,
    }
}

pub fn winrt_v2_from_cgmath_vec(v: cgmath::Vector2<f32>) -> Vector2 {
    Vector2 { X: v.x, Y: v.y }
}

pub fn winrt_v2_from_cgmath_pt(v: cgmath::Point2<f32>) -> Vector2 {
    Vector2 { X: v.x, Y: v.y }
}

pub fn winrt_color_from_rgbaf32(c: crate::iface::RGBAF32) -> Color {
    let f = |c: f32| (c.fmax(0.0).fmin(1.0) * 255.0) as u8;
    let c = c.map_rgb(f).map_alpha(f);
    Color {
        A: c.a,
        R: c.r,
        G: c.g,
        B: c.b,
    }
}
