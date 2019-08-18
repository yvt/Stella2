//! A bin rasterizer.
use arrayvec::ArrayVec;
use cgmath::{vec2, Vector2};
use itertools::izip;
use std::cmp::{max, min};
use zerocopy::LayoutVerified;

use super::{
    binner::{Binner, Bmp, Content, Elem, ElemFlags},
    CLIP_SUB, CLIP_SUB_SHIFT, NUM_LAYERS, TILE, UV_SUB, UV_SUB_SHIFT,
};

/// A working area for bin rasterization.
pub struct BinRast {
    /// Tile buffers for layers, continuously holding
    /// `TILE * TILE * 4 * NUM_LAYERS` bytes. The actual structure is like this:
    /// `[[[[u8; TILE]; TILE]; 4]; NUM_LAYERS]`.
    layers: Box<[[[u8; TILE * TILE]; 4]; NUM_LAYERS]>,
}

impl BinRast {
    pub fn new() -> Self {
        Self {
            layers: Box::new([[[0; TILE * TILE]; 4]; NUM_LAYERS]),
        }
    }

    /// Copy the result image (of size `TILE`²) to a supplied image buffer.
    ///
    /// For each `(x, y)` in range `0 ≤ x < clip_width` (`clip_width` must be
    /// ≤ `TILE`) and `0 ≤ y < clip_height` (`clip_height` must be ≤ `TILE`),
    /// the corresponding pixel in the result buffer is copied to
    /// `to[x + y * stride..][0..4]`.
    pub fn copy_to(&self, to: &mut [u8], stride: usize, clip_width: usize, clip_height: usize) {
        assert!(clip_width <= TILE);
        assert!(clip_height <= TILE);

        let src_layer = &self.layers[0];
        let [src_l0, src_l1, src_l2, src_l3] = src_layer;

        for y in 0..clip_height {
            let row_start = y * TILE;
            let row_range = row_start..row_start + clip_width;
            let src_row0 = &src_l0[row_range.clone()];
            let src_row1 = &src_l1[row_range.clone()];
            let src_row2 = &src_l2[row_range.clone()];
            let src_row3 = &src_l3[row_range.clone()];

            let to_row = &mut to[stride * y..][0..clip_width * 4];

            for (s0, s1, s2, s3, t) in izip!(
                src_row0,
                src_row1,
                src_row2,
                src_row3,
                to_row.chunks_exact_mut(4)
            ) {
                t[0] = *s0;
                t[1] = *s1;
                t[2] = *s2;
                t[3] = *s3;
            }
        }
    }

    /// Rasterize the bin in `binner`, specified by `bin_index`.
    pub fn rasterize<TBmp: Bmp>(&mut self, binner: &Binner<TBmp>, bin_index: [usize; 2]) {
        for (elem, layer) in binner.bin_elems(bin_index) {
            self.rasterize_elem(bin_index, elem, layer as usize);
        }
    }

    #[inline]
    fn rasterize_elem<TBmp: Bmp>(
        &mut self,
        bin_index: [usize; 2],
        elem: &Elem<TBmp>,
        layer: usize,
    ) {
        let bin_coords = [bin_index[0] * TILE, bin_index[1] * TILE];

        let sci = elem.scissor;

        // Clipped scissor rectangle (global)
        let sci_clip_g = [
            max(sci.min.x as u32, bin_coords[0] as u32),
            max(sci.min.y as u32, bin_coords[1] as u32),
            min(sci.max.x as u32, (bin_coords[0] + TILE) as u32),
            min(sci.max.y as u32, (bin_coords[1] + TILE) as u32),
        ];

        // Clipped scissor rectangle (local)
        let sci_clip_l = [
            sci_clip_g[0] - bin_coords[0] as u32,
            sci_clip_g[1] - bin_coords[1] as u32,
            sci_clip_g[2] - bin_coords[0] as u32,
            sci_clip_g[3] - bin_coords[1] as u32,
        ];

        // The layer buffers
        let (dest_layer, rest_layers) = self.layers[layer..].split_first_mut().unwrap();

        // Decompose `dest_layer` into channels. They usually represeent blue,
        // green, red, and alpha respectively.
        let [dest_l0, dest_l1, dest_l2, dest_l3] = dest_layer;

        // Content
        enum RastContent<'a> {
            Solid([u8; 4]),
            Bmp {
                bmp_data: &'a [[u8; 4]],
                bmp_size: [usize; 2],
                bmp_stride: usize,
            },
            Layer(&'a mut [[u8; TILE * TILE]; 4]),
        }

        let [mut uv_origin, mut duv_dx, mut duv_dy] = [vec2(0, 0); 3];

        let cont: RastContent<'_> = match elem.content {
            Content::Solid(x) => RastContent::Solid(x),

            Content::Bmp {
                ref bmp,
                uv_origin: uv_origin_,
                duv_dx: duv_dx_,
                duv_dy: duv_dy_,
            } => {
                let bmp_size = bmp.size();
                let bmp_stride = bmp.stride() / 4;
                let bmp_data = LayoutVerified::new_slice_unaligned(bmp.data())
                    .unwrap()
                    .into_slice();

                uv_origin = uv_origin
                    + duv_dx * (sci_clip_g[0] - sci.min.x as u32) as i32
                    + duv_dy * (sci_clip_g[1] - sci.min.y as u32) as i32;
                duv_dx = duv_dx_;
                duv_dy = duv_dy_;

                RastContent::Bmp {
                    bmp_data,
                    bmp_size,
                    bmp_stride,
                }
            }

            Content::Layer(src_layer) => {
                RastContent::Layer(&mut rest_layers[src_layer as usize - layer - 1])
            }
        };

        // Clip planes
        let cps = elem.clip_planes.clone();

        // TODO: Optimize

        // Draw pixels
        for (y_l, y_g, y) in izip!(sci_clip_l[1]..sci_clip_l[3], sci_clip_g[1].., 0..) {
            let row_start = y_l as usize * TILE;
            let row_range =
                (row_start + sci_clip_l[0] as usize)..(row_start + sci_clip_l[2] as usize);
            let dest_row0 = &mut dest_l0[row_range.clone()];
            let dest_row1 = &mut dest_l1[row_range.clone()];
            let dest_row2 = &mut dest_l2[row_range.clone()];
            let dest_row3 = &mut dest_l3[row_range.clone()];

            for (i, x, x_g, d0, d1, d2, d3) in izip!(
                row_range.start..,
                0..,
                sci_clip_g[1]..,
                dest_row0,
                dest_row1,
                dest_row2,
                dest_row3
            ) {
                let uv = uv_origin + duv_dy * y as i32 + duv_dx * x as i32;
                let mut cvs = cps
                    .iter()
                    .map(|cp| cp.n.x * x_g as i32 + cp.n.y * y_g as i32)
                    .collect::<ArrayVec<[_; 2]>>()
                    .into_inner()
                    .unwrap();

                // Get the content color value
                let mut c = match cont {
                    RastContent::Solid([c0, c1, c2, _]) => [c0 as u32, c1 as u32, c2 as u32, 255],

                    RastContent::Bmp {
                        bmp_data,
                        bmp_size,
                        bmp_stride,
                    } => sample_bilinear(bmp_data, bmp_size, bmp_stride, uv.into()),

                    RastContent::Layer(ref src_layer) => src_layer
                        .iter()
                        .map(|chan| chan[i] as u32)
                        .collect::<ArrayVec<[_; 4]>>()
                        .into_inner()
                        .unwrap(),
                };

                // Mask
                let mut mask;

                if elem.flags.contains(ElemFlags::CLIP_PLANES) {
                    // See `ClipPlanes` for what this code means..
                    if elem.flags.contains(ElemFlags::CLIP_PLANES_ANTIALIASED) {
                        mask = izip!(&cps, &cvs)
                            .map(|(cp, cv)| {
                                (integrate_step(cv - cp.d.start) - integrate_step(cv - cp.d.end))
                                    >> (CLIP_SUB_SHIFT - 8)
                            })
                            .fold(256, |x, y| (x * y as u32) / 256)
                            * 256;
                    } else {
                        mask = izip!(&cps, &cvs)
                            .map(|(cp, cv)| cp.d.contains(cv) as u32)
                            .fold(1, |x, y| x & y)
                            * 256;
                    }
                    debug_assert!(mask <= 256);

                    mask = (mask * elem.opacity as u32) / 256;
                } else {
                    mask = elem.opacity as u32;
                }

                debug_assert!(mask <= 256);

                // Apply the mask
                let c = [
                    (c[0] as u32 * mask) / 256,
                    (c[1] as u32 * mask) / 256,
                    (c[2] as u32 * mask) / 256,
                    (c[3] as u32 * mask) / 256,
                ];

                // Map the alpha value from `0..=255` to `0..=256`
                let alpha = c[3] + c[3] / 128;

                // Blend over (with premultiplied alpha)
                for (d, c) in izip!(&mut [d0, d1, d2, d3], &c) {
                    **d = min(*c + **d as u32 * (256 - alpha) / 256, 255) as u8;
                }
            }
        }
    }
}

/// Integrate `step(x)` over `x..x + CLIP_SUB`.
fn integrate_step(x: i32) -> i32 {
    max(x + CLIP_SUB, 0) - max(x, 0)
}

fn sample_bilinear(data: &[[u8; 4]], size: [usize; 2], stride: usize, uv: [i32; 2]) -> [u32; 4] {
    let [x1, y1] = [uv[0] >> UV_SUB_SHIFT, uv[1] >> UV_SUB_SHIFT];
    let [x2, y2] = [x1 + 1, y1 + 1];

    let clamp = |[x, y]: [i32; 2]| {
        [
            min(max(x, 0), size[0] as i32 - 1) as usize,
            min(max(y, 0), size[1] as i32 - 1) as usize,
        ]
    };
    let [x1, y1] = clamp([x1, y1]);
    let [x2, y2] = clamp([x2, y2]);

    let get = |x, y| {
        let [b, g, r, a]: [u8; 4] = data[x + y * stride];
        [b as u32, g as u32, r as u32, a as u32]
    };
    let [p0, p1] = [get(x1, y1), get(x2, y1)];
    let [p2, p3] = [get(x1, y2), get(x2, y2)];

    let p0 = lerp_color(p0, p1, (uv[0] & (UV_SUB - 1)) as u32);
    let p2 = lerp_color(p2, p3, (uv[0] & (UV_SUB - 1)) as u32);

    lerp_color(p0, p2, (uv[1] & (UV_SUB - 1)) as u32)
}

fn lerp_color(a: [u32; 4], b: [u32; 4], f: u32) -> [u32; 4] {
    izip!(&a, &b)
        .map(|(&a, &b)| (a * (UV_SUB as u32 - f) + b * f + UV_SUB as u32 / 2) / UV_SUB as u32)
        .collect::<ArrayVec<[_; 4]>>()
        .into_inner()
        .unwrap()
}
