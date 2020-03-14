use alt_fp::FloatOrd;
use cggeom::Box2;
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

/// Find the union of given boxes.
///
/// Assumes all inputs are finite.
pub fn union_box_f32(elems: impl IntoIterator<Item = Box2<f32>>) -> Option<Box2<f32>> {
    use packed_simd::f32x4;

    let edges = elems
        .into_iter()
        .fold(f32x4::splat(std::f32::NAN), |edges, bx| {
            edges.fmin(f32x4::new(bx.min.x, bx.min.y, -bx.max.x, -bx.max.y))
        });

    if edges.is_nan().any() {
        None
    } else {
        Some(Box2 {
            min: [edges.extract(0), edges.extract(1)].into(),
            max: [-edges.extract(2), -edges.extract(3)].into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cggeom::{box2, prelude::*};
    use quickcheck::TestResult;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn test_union_box_f32(coords: Vec<f32>) -> TestResult {
        let boxes = coords.chunks_exact(4).filter_map(|coords| {
            let bx = box2! { min: [coords[0], coords[1]], max: [coords[2], coords[3]] };
            if bx.is_empty() {
                None
            } else {
                Some(bx)
            }
        });

        let expected = boxes
            .clone()
            .fold(None, |x: Option<Box2<f32>>, y| match (x, y) {
                (Some(x), y) => Some(x.union(&y)),
                (None, y) => Some(y),
            });

        let actual = union_box_f32(boxes);

        if expected != actual {
            return TestResult::error(format!("expected = {:?}, got = {:?}", expected, actual));
        }

        TestResult::passed()
    }
}
