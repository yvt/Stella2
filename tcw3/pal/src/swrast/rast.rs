use checked::Checked;
use rayon::prelude::*;
use std::{cell::RefCell, cmp::min};

use super::{
    binner::{Binner, Bmp},
    binrast::BinRast,
    TILE,
};

thread_local! {
    static BIN_RAST: RefCell<BinRast> = RefCell::new(BinRast::new());
}

/// Rasterize the contents of `binner` into the specified image buffer.
///
/// Let `size` be `binner.target_size()`.  `out.len()` must be at least
/// `out_stride * (size[1] - 1) + size[0] * 4`.
///
/// `out_stride` must be at least `size[0] * 4`.
pub fn rasterize(binner: &Binner<impl Bmp>, out: &mut [u8], out_stride: usize) {
    let target_size = binner.target_size();
    let bin_count = binner.bin_count();

    if target_size[0] == 0 || target_size[1] == 0 {
        return;
    }

    let required_stride = Checked::from(target_size[0]) * 4;
    let required_stride = required_stride.expect("overflow");
    assert!(out_stride >= required_stride);

    let required_size = Checked::from(out_stride) * (target_size[1] - 1) + required_stride;
    let required_size = required_size.expect("overflow");
    assert!(out.len() >= required_size);

    // For each row of tiles...
    out.par_chunks_mut(out_stride * TILE)
        .enumerate()
        .take(bin_count[1])
        .for_each(|(y, out)| {
            BIN_RAST.with(|cell| {
                let mut bin_rast = cell.borrow_mut();
                let bin_h = min(TILE, target_size[1] - y * TILE);
                for x in 0..bin_count[0] {
                    let bin_w = min(TILE, target_size[0] - x * TILE);

                    bin_rast.rasterize(binner, [x, y]);
                    bin_rast.copy_to(&mut out[x * TILE * 4..], out_stride, bin_w, bin_h);
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::super::binner::ElemInfo;
    use super::*;

    use cggeom::box2;
    use cgmath::Matrix3;
    use quickcheck::TestResult;
    use quickcheck_macros::quickcheck;

    #[derive(Debug, Clone)]
    struct TestBmp;

    impl Bmp for TestBmp {
        fn data(&self) -> &[u8] {
            unreachable!()
        }
        fn size(&self) -> [usize; 2] {
            [40, 20]
        }
        fn stride(&self) -> usize {
            40
        }
    }

    #[quickcheck]
    fn smoke_test(size_x: usize, size_y: usize, extra_stride: usize) -> TestResult {
        let size = [size_x, size_y];
        // Limit the memory usage and the test execution time
        if size[0] > 400 || size[1] > 400 || extra_stride > 400 {
            return TestResult::discard();
        }

        let mut binner = Binner::<TestBmp>::new();
        {
            let mut builder = binner.build(size);

            let xform = Matrix3::new(5.5, 7.5, 0.0, 6.0, 4.5, 0.0, 10.0, 15.0, 1.0);
            builder.push_elem(ElemInfo {
                xform,
                bounds: box2! { min: [0.0, 0.0], max: [100.0, 100.0] },
                contents_center: box2! { min: [0.0, 0.0], max: [1.0, 1.0] },
                contents_scale: 1.0,
                bitmap: None,
                bg_color: [50, 80, 100, 200].into(),
                opacity: 0.8,
            });

            builder.finish();
        }

        let stride = size_x * 4 + extra_stride;
        let mut out_image = vec![
            0xffu8;
            if size_y == 0 {
                0
            } else {
                stride * (size_y - 1) + size_x * 4
            }
        ];

        if stride == 0 {
            return TestResult::discard();
        }

        rasterize(&binner, &mut out_image, stride);

        for (i, line) in out_image.chunks(stride).enumerate() {
            let inner = &line[0..size_x * 4];
            let outer = &line[size_x * 4..];

            if inner.iter().any(|x| *x == 0xff) {
                return TestResult::error(format!("Found an unfilled pixel in line {}", i));
            }

            if outer.iter().any(|x| *x != 0xff) {
                return TestResult::error(format!("Found a leaked pixel in line {}", i));
            }
        }

        TestResult::passed()
    }
}
