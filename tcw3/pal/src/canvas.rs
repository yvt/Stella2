//! Provides default implementation for some methods of `Canvas`.
//!
//! These method are used as a provided implementation of `Canvas`'s methods.
//! They are not written in-line but instead defined here because they are
//! lengthy and make `iface.rs` harder to read.
use cggeom::{prelude::*, Box2};
use cgmath::Point2;

use super::iface::Canvas;

pub fn canvas_rect(canvas: &mut (impl Canvas + ?Sized), bx: Box2<f32>) {
    canvas.move_to(Point2::new(bx.min.x, bx.min.y));
    canvas.line_to(Point2::new(bx.max.x, bx.min.y));
    canvas.line_to(Point2::new(bx.max.x, bx.max.y));
    canvas.line_to(Point2::new(bx.min.x, bx.max.y));
    canvas.close_path();
}

pub fn canvas_rounded_rect(
    canvas: &mut (impl Canvas + ?Sized),
    bx: Box2<f32>,
    mut radii: [[f32; 2]; 4],
) {
    // Handle overlapping corners
    use alt_fp::FloatOrdSet;
    let size = bx.size();
    let overlap_factor = [
        size.x / (radii[0][0] + radii[1][0]),
        size.x / (radii[2][0] + radii[3][0]),
        size.y / (radii[1][1] + radii[2][1]),
        size.y / (radii[0][1] + radii[3][1]),
    ]
    .fmin();
    let overlap_factor = [overlap_factor, 1.0].fmin();
    for corner in radii.iter_mut() {
        corner[0] *= overlap_factor;
        corner[1] *= overlap_factor;
    }

    // The control point position for approximating a circuler arc using
    // a cubic Bézier curve.
    // See <https://pomax.github.io/bezierinfo/#circles_cubic> for derivation.
    const CIRCLE_CP: f32 = 0.55228;
    // The control points relative to each corner. Actually, this is
    // `[[[f32; 2]; 4]; 4]` but the last level is flattened to make the SIMD
    // implementation easy
    #[rustfmt::skip]
    const CPS: [[f32; 8]; 4] = [
        [
            0.0, 1.0,
            0.0, 1.0 - CIRCLE_CP,
            1.0 - CIRCLE_CP, 0.0,
            1.0, 0.0,
        ],
        [
            -1.0, 0.0,
            -1.0 + CIRCLE_CP, 0.0,
            0.0, 1.0 - CIRCLE_CP,
            0.0, 1.0,
        ],
        [
            0.0, -1.0,
            0.0, -1.0 + CIRCLE_CP,
            -1.0 + CIRCLE_CP, 0.0,
            -1.0, 0.0,
        ],
        [
            1.0, 0.0,
            1.0 - CIRCLE_CP, 0.0,
            0.0, -1.0 + CIRCLE_CP,
            0.0, -1.0,
        ],
    ];

    use alt_fp::fma;
    use array::Array;
    use packed_simd::{f32x4, shuffle};

    let corners = [
        f32x4::new(bx.min.x, bx.min.y, radii[0][0], radii[0][1]),
        f32x4::new(bx.max.x, bx.min.y, radii[1][0], radii[1][1]),
        f32x4::new(bx.max.x, bx.max.y, radii[2][0], radii[2][1]),
        f32x4::new(bx.min.x, bx.max.y, radii[3][0], radii[3][1]),
    ];

    for i in 0..4 {
        // Calculate the absolute position of the control points

        // let corner = corners[i];
        // let corner_radii = radii[i];
        // CPS[i].map(|[x, y]| {
        //     [
        //         fma![(corner[0]) + x * (corner_radii[0])],
        //         fma![(corner[1]) + y * (corner_radii[1])],
        //     ]
        // });

        // The compiler won't vectorize the above code, so we vectorize it
        // manually. This brings a moderate code size reduction.
        let corner_info = corners[i];
        let corner = shuffle!(corner_info, [0, 1, 0, 1]);
        let corner_radii = shuffle!(corner_info, [2, 3, 2, 3]);

        let corner_cps: [f32x4; 2] = Array::from_fn(|k| {
            let relative_cps = f32x4::from_slice_unaligned(&CPS[i][k * 4..]);

            fma![corner + relative_cps * corner_radii]
        });

        let corner_cps = flatten_f32x4x2(corner_cps);

        // Issue draw commands
        if i == 0 {
            canvas.move_to(Point2::new(corner_cps[0], corner_cps[1]));
        } else {
            canvas.line_to(Point2::new(corner_cps[0], corner_cps[1]));
        }
        canvas.cubic_bezier_to(
            Point2::new(corner_cps[2], corner_cps[3]),
            Point2::new(corner_cps[4], corner_cps[5]),
            Point2::new(corner_cps[6], corner_cps[7]),
        );
    }

    canvas.close_path();
}

#[inline]
fn flatten_f32x4x2(x: [packed_simd::f32x4; 2]) -> [f32; 8] {
    [
        x[0].extract(0),
        x[0].extract(1),
        x[0].extract(2),
        x[0].extract(3),
        x[1].extract(0),
        x[1].extract(1),
        x[1].extract(2),
        x[1].extract(3),
    ]
}

pub fn canvas_ellipse(canvas: &mut (impl Canvas + ?Sized), bx: Box2<f32>) {
    let radius = [bx.size().x * 0.5, bx.size().y * 0.5];
    canvas_rounded_rect(canvas, bx, [radius; 4]);
}
