//! Binner (not "bin"ary!)
use alt_fp::{FloatOrd, FloatOrdSet};
use arrayvec::ArrayVec;
use bitflags::bitflags;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{prelude::*, Matrix3, Point2, Vector2};
use flags_macro::flags;
use itertools::iproduct;
use rgb::RGBA8;
use std::{
    cmp::{max, min},
    ops::Range,
};

use super::{CLIP_SUB, NUM_GROUPS, NUM_LAYERS, TILE, UV_SUB};

/// A temporary storage for binning.
#[derive(Debug)]
pub struct Binner<TBmp> {
    bins: Vec<Bin>,
    elems: Vec<Elem<TBmp>>,
    frags: Vec<Frag>,
    target_size: [usize; 2],
    bin_count: [usize; 2],

    /// Used by `BinnerBuilder`. For each group, the corresponding element
    /// stores the list of bins that have a compositing fragment for the group.
    ///
    /// Invalid for groups that don't have their own layers
    /// (i.e., their `elem_i` is `None`).
    group_bins: [Vec<u32>; NUM_GROUPS],
}

/// A clonable reference to a color-matched bitmap image.
pub trait Bmp: Send + Sync + Clone + 'static {
    fn data(&self) -> &[u8];
    fn size(&self) -> [usize; 2];
    fn stride(&self) -> usize;
}

#[derive(Debug, Clone, Copy)]
struct Bin {
    /// The first fragment in the bin's fragment list, or `NONE`.
    ///
    /// The fragment list is processed in a drawing order (back to front).
    frag_first_i: u32,
    /// The last fragment in the bin's fragment list, or `NONE`.
    frag_last_i: u32,

    /// Used by `BinnerBuilder`.
    ///
    /// For all `i` such that `i < max_layered_group_i`, the bin has a compositing
    /// fragment for the group `groups[i]`, or `groups[i].layer.is_none()`.
    ///
    /// Also, if `max_layered_group_i > 0`, then
    /// `groups[max_layered_group_i - 1].layer` must be `Some(_)`.
    max_layered_group_i: u32,

    _pad: u32,
}

/// A rendered element, referenced by one or more fragments
#[derive(Debug)]
struct Elem<TBmp> {
    flags: ElemFlags,

    /// Opacity in range `0..=256`.
    opacity: u16,

    content: Content<TBmp>,

    /// The scissor rectangle.
    scissor: Box2<u16>,

    /// Clip planes (enabled by `ElemFlags::CLIP_PLANES`).
    clip_planes: [ClipPlanes; 2],
}

/// Clip planes (enabled by `ElemFlags::CLIP_PLANES`). Given window
/// pixel coordinates `p = vec2(x, y)`, the pixel is considered included if
/// `d.contains(&p.dot(n)))`. The length of `n` must be close to `CLIP_SUB` for
/// edge antialiasing to work.
///
/// We wish to rasterize an parallelogram with antialiasing. The naïve
/// derivation is, for each output pixel, to integrate a square region in a
/// 2D function representing the shape to get the coverage value, but this
/// approach is complicated and probably slow. Thus we rely on approximation.
///
/// A parallelogram is an intersection of two thick straight lines, each of
/// which is in turn an intersection of two half planes sharing the same
/// normal vector. Based on this observation, we approximate the calculation
/// of coverage values as follows:
///
///  - A coverage value of a shape formed by an intersection of two shapes
///    is calculated as a product of coverage values calculated for the
///    the individual shapes. Thus, the final coverage value is a product
///    of the four half planes' coverage values.
///
///  - A coverage value of a half plane is a clamped linear function of
///    sample coordinates. (In reality, it's a piecewise linear function.)
///    The width of the function's transition zone has a size of a pixel.
///
#[derive(Debug, Clone)]
struct ClipPlanes {
    n: Vector2<i32>,
    d: Range<i32>,
}

impl Default for ClipPlanes {
    fn default() -> Self {
        Self {
            n: [0, 0].into(),
            d: 0..0,
        }
    }
}

#[derive(Debug, Clone)]
enum Content<TBmp> {
    /// A solid color, The alpha channel is ignored and taken from
    /// `Elem::opacity`.
    Solid([u8; 4]),

    /// A bitmap image.
    Bmp {
        bmp: TBmp,
        /// ```text
        /// uv = (uv_origin + duv_dx * (x - scissor.min.x)
        ///                 + duv_dy * (y - scissor.min.y) >> UV_SUB_SHIFT
        /// (uv represents unnormalized bitmap coordinates)
        /// ```
        uv_origin: Vector2<i32>,
        duv_dx: Vector2<i32>,
        duv_dy: Vector2<i32>,
    },

    /// Use the content of another layer.
    ///
    /// `Elem` and `Frag` including this are called a compositing element and
    /// fragment, respectively. A fragment stream using this looks like the
    /// following:
    ///
    /// ```text
    ///   - Frag { layer = 0, elem = ... }
    ///   - Frag { layer = 1, elem = ... }
    ///   - Frag { layer = 0, elem = { content = Layer(1) } }
    /// ```
    ///
    /// **Using this automatically clears the source layer.**
    Layer(u8),
}

bitflags! {
    struct ElemFlags: u8 {
        /// Enable clipping by clip planes
        ///
        /// Clip planes are more flexible than a scissor rectangle, but render
        /// slower. Hence, we avoid clip planes whenever possible. We don't
        /// cull the rendering of regions masked by clip planes.
        const CLIP_PLANES = 1 << 0;

        /// Enable antialiased clipping by clip planes. Requires `CLIP_PLANES`.
        ///
        ///  - Antialiased clipping is useful for implementing `MASK_TO_BOUNDS`
        ///    as well as for shaping the outline of a non-axis-aligned layer.
        ///
        ///  - Aliased clipping is useful for rendering slices of 9-slice
        ///    scaling.
        ///
        const CLIP_PLANES_ANTIALIASED = 1 << 1;
    }
}

/// A rendered fragment - A drawing command for a single bin.
#[derive(Debug)]
struct Frag {
    /// Index of `Elem`.
    elem_i: u32,

    /// The next index of `Frag`. `NONE` if there is none.
    next_frag_i: u32,

    /// The 0-based layer number. Layers are like CPU registers and can be used
    /// to implement complex composite operations. Layer 0 is used as output.
    /// The valid range is `0..NUM_LAYERS`.
    layer: u8,
}

const NONE: u32 = 0xffffffff;

impl<TBmp: Bmp> Binner<TBmp> {
    /// Construct a `Binner`.
    pub fn new() -> Self {
        Self {
            bins: Vec::new(),
            elems: Vec::new(),
            frags: Vec::new(),
            target_size: [0, 0],
            bin_count: [0, 0],
            group_bins: Default::default(),
        }
    }

    /// Initialize the storage to accomodate the specified render target size,
    /// and start filling bins.
    pub(super) fn build(&mut self, size: [usize; 2]) -> BinnerBuilder<'_, TBmp> {
        assert!(size[0] <= <u16>::max_value() as usize);
        assert!(size[1] <= <u16>::max_value() as usize);

        self.frags.clear();
        self.elems.clear();
        self.bins.clear();

        self.target_size = size;
        self.bin_count = [
            (size[0] + TILE as usize - 1) / TILE as usize,
            (size[1] + TILE as usize - 1) / TILE as usize,
        ];

        self.bins
            .extend((0..self.bin_count[0] * self.bin_count[1]).map(|_| Bin {
                frag_first_i: NONE,
                frag_last_i: NONE,
                max_layered_group_i: 0,
                _pad: 0,
            }));

        BinnerBuilder {
            binner: self,
            groups: ArrayVec::new(),
            top_layered_group_i: 0,
            layer: 0,
            scissor: Some(box2! {
                min: [0, 0],
                max: [size[0] as u16, size[1] as u16],
            }),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ElemInfo<TBmp> {
    pub xform: Matrix3<f32>,
    pub bounds: Box2<f32>,
    pub contents_center: Box2<f32>,
    pub contents_scale: f32,
    pub bitmap: Option<TBmp>,
    /// The background color.
    pub bg_color: RGBA8,
    pub opacity: f32,
}

/// This type is used to add rendered elements to `Binner`.
///
/// The methods should be called in a reverse drawing order (front to back).
/// This is to skip the processing of fragments which are completely occluded by
/// fragments in the front.
#[derive(Debug)]
pub(super) struct BinnerBuilder<'a, TBmp> {
    binner: &'a mut Binner<TBmp>,

    /// The stack of active groups.
    ///
    /// The content of the group at index `i` is drawn on layer `i + 1`.
    groups: ArrayVec<[BuilderGr; NUM_GROUPS]>,

    /// The last element + 1 in `groups` having `GrLayer`.
    top_layered_group_i: u32,

    /// The current layer. It starts at `0`.
    ///
    /// It's equal to: `groups.iter().map(|g| g.layer.is_some() as usize).sum()`.
    /// Also, if `top_layered_group_i != 0`, it's equal to `GrLayer::layer` of
    /// the group at index `top_layered_group_i - 1`.
    layer: u8,

    /// The current scissor rectangle. Initially, this represents the entirety
    /// of the render target. `None` means empty.
    scissor: Option<Box2<u16>>,
}

/// A builder group.
#[derive(Debug)]
struct BuilderGr {
    /// If the group has a layer, this stores the layer information.
    layer: Option<GrLayer>,

    /// The old scissor rectangle to be restored when this group is closed.
    old_scissor: Option<Box2<u16>>,
}

#[derive(Debug)]
struct GrLayer {
    /// Points `Elem` that is used to composite this layer onto the parent
    /// layer (`layer - 1`).
    ///
    /// The reason we store this is that we want to defer the generation of
    /// compositing fragments so that we can skip it for bins don't include
    /// the layer's content. This is especially important when `mask_xform`
    /// is `None` because, in this case, the layer encompasses entire the
    /// render target even if its content occupies a small portion of it.
    elem_i: u32,

    /// The layer number.
    layer: u8,
}

impl<TBmp: Bmp> BinnerBuilder<'_, TBmp> {
    pub(super) fn finish(self) {
        assert!(self.groups.is_empty(), "All non-root groups must be closed");
        assert_eq!(self.layer, 0);
    }

    /// Open a composition group.
    ///
    /// `mask_xform` describes the mask shape of the group. A square shape
    /// `0 ≤ x, y ≤ 1` is transformed using this transformation matrix. The last
    /// row of the matrix must be `[0 0 1]`. The resulting parallelogram is
    /// used as the mask shape. Masking is not applied if it's `None`.
    ///
    /// `opacity` is the opacity of the group in range `0.0..=1.0`. The opacity
    /// is applied after all elements in the group are flattened into a single
    /// image.
    pub(super) fn open_group(&mut self, mask_xform: Option<Matrix3<f32>>, opacity: f32) {
        // The last row of the matrix must be `[0 0 1]`.
        if let Some(xform) = mask_xform {
            debug_assert!(is_affine_xform(xform));
        }

        assert!(
            self.groups.len() < self.groups.capacity(),
            "Too many groups"
        );

        // Closure as a scope for `?`
        let new_scissor: Option<Box2<u16>> = (|| {
            let old_scissor = self.scissor?;

            if let Some(xform) = mask_xform {
                // Calculate the AABB of the mask shape
                let unit_sq = box2! { min: [0.0; 2], max: [1.0; 2] };
                let bb = xform_aabb(xform, unit_sq);

                let bb = saturating_aabb_f32_to_u16(round_aabb_conservative(bb))?;

                bb.intersection(&old_scissor)
            } else {
                Some(old_scissor)
            }
        })();

        if new_scissor.is_none() {
            // If the new scissor rectangle is empty, the group doesn't need a
            // layer.
            self.groups.push(BuilderGr {
                layer: None,
                old_scissor: self.scissor,
            });
            self.scissor = None;
            return;
        }

        let clip_planes = mask_xform.map(xform_to_clip_planes);

        let needs_clip_planes = if let Some(clip_planes) = &clip_planes {
            // If the mask shape isn't an axis-aligned box that is perfectly
            // aligned to pixels, the group needs a layer. Also, the compositing
            // `Elem` should use clip planes for masking.
            !clip_planes.iter().all(is_clip_planes_aligned_to_pixel)
        } else {
            false
        };

        // f `opacity` is less than `1`, the group needs a layer.
        let needs_layer = needs_clip_planes || opacity < 1.0;

        // Create a compositing `Elem` if needed
        let layer = if needs_layer {
            let elem_i = self.binner.elems.len() as u32;

            assert!(((self.layer + 1) as usize) < NUM_LAYERS, "Too many layers");
            self.layer += 1;

            self.binner.elems.push(Elem {
                flags: if needs_clip_planes {
                    flags![ElemFlags::{CLIP_PLANES | CLIP_PLANES_ANTIALIASED}]
                } else {
                    flags![ElemFlags::{}]
                },
                opacity: (opacity * 256.0) as u16,
                content: Content::Layer(self.layer as u8),
                scissor: new_scissor.unwrap(),
                clip_planes: clip_planes.unwrap_or_default(),
            });

            // We don't generate compositing `Frag`s at this point. That happens
            // when we add a content to a bin for the first time.
            // `Bin::max_layered_group_i` is used to track whether this happened
            // or not.

            self.top_layered_group_i = self.groups.len() as u32 + 1;

            Some(GrLayer {
                elem_i,
                layer: self.layer,
            })
        } else {
            None
        };

        self.groups.push(BuilderGr {
            layer,
            old_scissor: self.scissor,
        });
        self.scissor = new_scissor;
    }

    /// Close a composition group and insert it to the parent group.
    pub(super) fn close_group(&mut self) {
        let group = self.groups.pop().expect("Cannot close the root group");
        // Note: `self.layer` can be `0` if the group doesn't generate a layer
        let group_i = self.groups.len();

        if let Some(gr_layer) = group.layer {
            // This group has a layer.
            // Recalculate `top_layered_group_i` and `layer`
            debug_assert_eq!(self.top_layered_group_i, group_i as u32 + 1);
            debug_assert_eq!(self.layer, gr_layer.layer);
            self.top_layered_group_i = if let Some((i, _)) = self
                .groups
                .iter()
                .enumerate()
                .rev()
                .find(|(_, g)| g.layer.is_some())
            {
                i as u32 + 1
            } else {
                0
            };
            self.layer -= 1;

            // Pop `Bin::max_layered_group_i`
            let new_max_group_i = self.top_layered_group_i;

            for bin_i in self.binner.group_bins[group_i].drain(..) {
                let bin = &mut self.binner.bins[bin_i as usize];
                debug_assert_eq!(bin.max_layered_group_i, (group_i + 1) as u32);
                bin.max_layered_group_i = new_max_group_i;
            }
        } else {
            debug_assert_ne!(self.top_layered_group_i, group_i as u32 + 1);
        }

        self.scissor = group.old_scissor;
    }

    /// Insert an element.
    pub(super) fn push_elem(&mut self, info: ElemInfo<TBmp>) {
        debug_assert!(is_affine_xform(info.xform));

        if self.scissor.is_none() {
            return;
        }

        // Get geometric properties
        // TODO: See if this blows up with an empty space
        let xform = info.xform;
        let par = xform_and_aabb_to_parallelogram(xform, info.bounds);
        let bb = parallelogram_aabb(par);
        let clip_planes = xform_to_clip_planes(par);

        // The AABB of the rendered region
        let bb = if let Some(bb) = saturating_aabb_f32_to_u16(round_aabb_conservative(bb))
            .and_then(|bb| bb.intersection(&self.scissor.unwrap()))
        {
            bb
        } else {
            return;
        };

        // If all edges are aligned to pixels, we can omit edge antialiasing
        let aligned_to_pixel = clip_planes.iter().all(is_clip_planes_aligned_to_pixel);

        // If all edges are aligned to axes, 9-slice scaling can be implemented
        // by limiting the rendering regions of slices using scissor rectangles.
        // Otherwise, we need aliased clip planes for slicing.
        let aligned_to_axis = clip_planes.iter().all(is_clip_planes_aligned_to_axis);

        // `aligned_pixel` entails `aligned_to_axis`.
        debug_assert!(!aligned_to_pixel || aligned_to_axis);

        // Resolve the split positions for 9-slice scaling.
        struct SliceInfo {
            ///
            out_ct_center: Box2<f32>,

            /// Each bit indicates whether the corresponding row/column is
            /// empty in the target space.
            valid_slices: u8,

            /// The split positions for 9-slice scaling in the input bitmap's
            /// coordinate space
            in_crds: [[f32; 4]; 2],
        }

        impl SliceInfo {
            fn in_rect(&self, s_x: usize, s_y: usize) -> Box2<f32> {
                box2! {
                    min: [self.in_crds[0][s_x], self.in_crds[1][s_y]],
                    max: [self.in_crds[0][s_x + 1], self.in_crds[1][s_y + 1]],
                }
            }
        }

        let size = info.bounds.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }
        let slice_info = info.bitmap.as_ref().map(|bmp| {
            let ct_center = info.contents_center;
            let ct_scale = info.contents_scale;

            debug_assert!(ct_center.min.x >= 0.0 && ct_center.min.y >= 0.0);
            debug_assert!(ct_center.min.x <= ct_center.max.x);
            debug_assert!(ct_center.min.y <= ct_center.max.y);
            debug_assert!(ct_center.max.x <= 1.0 && ct_center.max.y <= 1.0);

            let bmp_size = bmp.size();
            let bmp_size = [bmp_size[0] as f32 / ct_scale, bmp_size[1] as f32 / ct_scale];

            let left = ct_center.min.x * bmp_size[0];
            let top = ct_center.min.y * bmp_size[1];
            let right = (1.0 - ct_center.max.x) * bmp_size[0];
            let bottom = (1.0 - ct_center.max.y) * bmp_size[1];

            let center_x = if left + right <= size.x {
                [left, (size.x - right).fmax(left)]
            } else {
                [size.x * (left / (left + right)); 2]
            };
            let center_y = if top + bottom <= size.y {
                [top, (size.y - bottom).fmax(top)]
            } else {
                [size.y * (top / (top + bottom)); 2]
            };

            debug_assert!([0.0, center_x[0], center_x[1], size.x].is_sorted());
            debug_assert!([0.0, center_y[0], center_y[1], size.y].is_sorted());

            // The origin is `info.bounds.min` (top left corner)
            let out_ct_center = box2! {
                min: [center_x[0], center_y[0]],
                max: [center_x[1], center_y[1]],
            };

            // Each bit indicates whether the corresponding row/column is
            // empty or not. Combined with the last assertions, the following
            // property is held: A row/column is empty iff its height/width is
            // zero.
            let valid_slices = u8::from(0.0 < center_x[0])
                | u8::from(center_x[0] < center_x[1]) << 1
                | u8::from(center_x[1] < size.x) << 2
                | u8::from(0.0 < center_y[0]) << 4
                | u8::from(center_y[0] < center_y[1]) << 5
                | u8::from(center_y[1] < size.y) << 6;

            // The split positions in the input bitmap's coordinate space
            let in_size = bmp.size();
            let in_size = [in_size[0] as f32, in_size[1] as f32];

            let in_ct_center = box2! {
                min: [ct_center.min.x * in_size[0], ct_center.min.y * in_size[1]],
                max: [ct_center.max.x * in_size[0], ct_center.max.y * in_size[1]],
            };

            let in_crds = [
                [0.0, in_ct_center.min.x, in_ct_center.max.x, in_size[0]],
                [0.0, in_ct_center.min.y, in_ct_center.max.y, in_size[1]],
            ];

            SliceInfo {
                out_ct_center,
                valid_slices,
                in_crds,
            }
        });

        let num_slices = if let Some(info) = &slice_info {
            (info.valid_slices & 0xf).count_ones() * (info.valid_slices & 0xf0).count_ones()
        } else {
            1
        };

        // If there are more than one (non-empty) slice, the slices must be
        // connected seamlessly.
        debug_assert!(num_slices <= 9);
        let use_slicing = match num_slices {
            0 => return,
            1 => false,
            _ => true,
        };

        // For non-axis-aligend elements, slicing is realized using aliased clip
        // planes, but they cannot co-exist with edge antialiasing. (There is
        // only one flag for clip plane antialiasing for each element.) Thus,
        // we combine all slices in a proxy layer, and then composite this layer
        // back to the original layer with a edge antialiasing mask.
        //
        // Also, if `info` has both of a non-transparent background color and
        // a bitmap, and `opacity` is less than `1`, we need a proxy in this
        // case too.
        // (Alternatively, we could add a new `Content` item to handle this
        // case, but that will increase the code size with few benefits.)
        let use_proxy = use_slicing && !aligned_to_axis
            || info.bg_color.a > 0 && info.opacity < 1.0 && info.bitmap.is_some();

        // -------------------------------------------------------------------
        // Analysis is mostly done, now it's a time to emit things.
        let mut elems: ArrayVec<[Elem<TBmp>; 10]> = ArrayVec::new();

        if let (Some(slice_info), Some(bmp)) = (slice_info, info.bitmap) {
            // silicing  aligned_pixel  aligned_to_axis  use_proxy |
            //                                                     | CP CPAA
            //                                               x     |
            //                                 x                   | CP CPAA
            //                                 x             x     |
            //                 x                                   | N/A
            //                 x                             x     | N/A
            //                 x               x                   |
            //                 x               x             x     |
            //     x                                               | N/A
            //     x                                         x     | CP
            //     x                           x                   | CP CPAA
            //     x                           x             x     |
            //     x           x                                   | N/A
            //     x           x                             x     | N/A
            //     x           x               x                   |
            //     x           x               x             x     |
            //
            // `slice_by_clip`: Elements' clip planes are used for slicing.
            //      If `false`, they are used for drawing the antialiased edge
            //      of the input element (`info`).
            const EMPTY: ElemFlags = ElemFlags::empty();
            const CLIP_PLANES: ElemFlags = ElemFlags::CLIP_PLANES;
            const CLIP_PLANES_AA: ElemFlags = ElemFlags::CLIP_PLANES_ANTIALIASED;
            let (elem_flags, slice_by_clip) =
                match [use_slicing, aligned_to_pixel, aligned_to_axis, use_proxy] {
                    // If the outer edge is aligned to pixels, clip planes
                    // aren't needed at all, and slicing can be done
                    // using scissor rectangles.
                    [_, true, true, _] => (EMPTY, false),

                    // Impossible cases: `aligned_pixel` must entail `aligned_to_axis`.
                    [_, true, false, _] => unreachable!(),

                    // If the outer edge is aligned to axes but not to pixels, the
                    // outer edge has to be antialiased, but slicing can be done
                    // using scissor rectangles.
                    [_, false, true, false] => (CLIP_PLANES | CLIP_PLANES_AA, false),
                    // .., but, the edge antialiasing could be done by
                    // the proxy instead.
                    [_, false, true, true] => (EMPTY, false),

                    // Impossible case: A single element can't have both
                    // antialiased and aliased clip planes! This case is already
                    // excluded by the definition of `use_proxy`.
                    [true, false, false, false] => unreachable!(),

                    // Elements use aliased clip planes for slicing. The proxy
                    // handles the outer edge.
                    [true, false, false, true] => (CLIP_PLANES, true),

                    // No slicing. The single element does edge antialiasing.
                    [false, false, false, false] => (CLIP_PLANES | CLIP_PLANES_AA, false),

                    // No slicing. The proxy does edge antialiasing.
                    [false, false, false, true] => (EMPTY, false),
                };

            use array::*;

            let valid_slices: ArrayVec<[_; 9]> = iproduct!(0..3, 0..3)
                .filter(|&(x, y)| {
                    let slice_valid_x = (slice_info.valid_slices & (1 << x)) != 0;
                    let slice_valid_y = (slice_info.valid_slices & (16 << y)) != 0;
                    slice_valid_x && slice_valid_y
                })
                .collect();

            let inv_xform = if let Some(x) = xform.invert() {
                x
            } else {
                // If the matrix is non-invertible, then the output region
                // is empty
                return;
            };

            let pre_xform_x = [
                info.bounds.min.x,
                info.bounds.min.x + slice_info.out_ct_center.min.x,
                info.bounds.min.x + slice_info.out_ct_center.max.x,
                info.bounds.max.x,
            ];
            let pre_xform_y = [
                info.bounds.min.y,
                info.bounds.min.y + slice_info.out_ct_center.min.y,
                info.bounds.min.y + slice_info.out_ct_center.max.y,
                info.bounds.max.y,
            ];

            // Transformation matrices from render target coordinates to
            // UV coordinates
            let uv_xforms: ArrayVec<[_; 9]> = valid_slices
                .iter()
                .map(|&(s_x, s_y)| {
                    let in_rect = slice_info.in_rect(s_x, s_y);
                    let px_x = &pre_xform_x[s_x..][0..2];
                    let px_y = &pre_xform_y[s_y..][0..2];

                    Matrix3::from_translation([in_rect.min.x, in_rect.min.y].into())
                        * Matrix3::from_nonuniform_scale_2d(
                            (in_rect.max.x - in_rect.min.x) / (px_x[1] - px_x[0]),
                            (in_rect.max.y - in_rect.min.y) / (px_y[1] - px_y[0]),
                        )
                        * Matrix3::from_translation([-px_x[0], -px_y[0]].into())
                        * inv_xform
                })
                .collect();

            let opacity = if use_proxy {
                256
            } else {
                (info.opacity * 256.0) as u16
            };

            if slice_by_clip {
                debug_assert!(!aligned_to_axis);

                // Using `xform_aabb` on individual slice might cause cracks
                // between slices due to numerical errors. So evaluate all
                // inner points and calculate bounding boxes from those.
                let xform = info.xform;
                let points: [[Point2<f32>; 4]; 4] = Array::from_fn(|x| {
                    Array::from_fn(|y| {
                        xform.transform_point([pre_xform_x[x], pre_xform_y[y]].into())
                    })
                });

                let clip_plane_normals: ArrayVec<[_; 2]> =
                    clip_planes.iter().map(|cp| cp.n).collect();
                let clip_plane_dists: ArrayVec<[_; 2]> = clip_planes
                    .iter()
                    .enumerate()
                    .map(|(i, clip_plane)| {
                        let points = [
                            points[if i == 0 { 1 } else { 0 }][if i == 1 { 1 } else { 0 }],
                            points[if i == 0 { 2 } else { 0 }][if i == 1 { 3 } else { 0 }],
                        ];
                        let dist = |p: Point2<f32>| {
                            let n = clip_plane.n;
                            (n.x as f32 * p.x as f32 + n.y as f32 * p.y as f32)
                                .fmax(<i32>::min_value() as f32)
                                .fmin(<i32>::max_value() as f32) as i32
                        };
                        [
                            <i32>::min_value(),
                            dist(points[0]),
                            dist(points[1]),
                            <i32>::max_value(),
                        ]
                    })
                    .collect();

                // For each slice...
                for (&(s_x, s_y), &uv_xform) in valid_slices.iter().zip(uv_xforms.iter()) {
                    let s1 = [s_x, s_y];
                    let s2: ArrayVec<[_; 2]> = s1
                        .iter()
                        .enumerate()
                        .map(|(i, &s1)| {
                            // Find the next valid row in range `s + 1..4`
                            let valid_slices = (slice_info.valid_slices >> i * 4) & 0b111;
                            let s2 = [3, 0, 1, 0, 2, 0, 1, 0]
                                [(valid_slices & !((2u8 << s1) - 1u8)) as usize];
                            debug_assert!(s2 > s1 && s2 < 4);
                            s2
                        })
                        .collect();

                    let clip_planes: [_; 2] = Array::from_fn(|i| ClipPlanes {
                        n: clip_plane_normals[i],
                        d: clip_plane_dists[i][s1[i]]..clip_plane_dists[i][s2[i]],
                    });

                    // Calculate the scissor rectangle from the corner points
                    let corners = [
                        points[s1[0]][s1[1]],
                        points[s2[0]][s1[1]],
                        points[s1[0]][s2[1]],
                        points[s2[0]][s2[1]],
                    ];

                    let scissor: Box2<f32> = box2! {
                        min: [
                            corners.map(|p| p.x).fmin(),
                            corners.map(|p| p.y).fmin(),
                        ],
                        max: [
                            corners.map(|p| p.x).fmax(),
                            corners.map(|p| p.y).fmax(),
                        ],
                    };
                    let scissor = if let Some(x) =
                        saturating_aabb_f32_to_u16(round_aabb_conservative(scissor))
                            .and_then(|scissor| scissor.intersection(&bb))
                    {
                        x
                    } else {
                        continue;
                    };

                    let content = Content::from_bmp(bmp.clone(), uv_xform, scissor);

                    elems.push(Elem {
                        flags: elem_flags,
                        opacity,
                        content,
                        scissor,
                        clip_planes,
                    });
                }
            } else if !use_slicing {
                // `slice_by_clip == false && use_slicing == false`
                let uv_xform = uv_xforms.first().unwrap();

                let scissor = bb;
                let content = Content::from_bmp(bmp.clone(), *uv_xform, scissor);

                elems.push(Elem {
                    flags: elem_flags,
                    opacity,
                    content,
                    scissor,
                    clip_planes: clip_planes.clone(),
                });
            } else {
                // `slice_by_clip == false && use_slicing == true`
                debug_assert!(aligned_to_axis);

                // Indicates whether `xform` involves axis transposition.
                //
                //        axis_swap = false:     axis_swap = true:
                //           ┌             ┐        ┌             ┐
                //           │ m00  0  m02 │        │  0  m01 m02 │
                //  xform    │  0  m11 m12 │        │ m10  0  m12 │
                //           │  0   0   1  │        │  0   0   1  │
                //           └             ┘        └             ┘
                //
                let axis_swap = clip_planes[0].n.x == 0;
                let ax0 = axis_swap as usize;
                let ax1 = 1 - ax0;

                // The destination rectangle is divided into 3×3 subrectangles.
                // Some of them are possibly empty.
                let bounds = info.bounds;
                let post_xform_crds = [
                    // The X coordinates of the split positions (sorted by an asencending
                    // order of the input `ax0` coordinates):
                    [
                        0.0,
                        slice_info.out_ct_center.min[ax0],
                        slice_info.out_ct_center.max[ax0],
                        size[ax0],
                    ]
                    .map(|x| (x + bounds.min[ax0]) * xform[ax0].x + xform.z.x),
                    // The Y coordinates of the split positions (sorted by an asencending
                    // order of the input `ax1` coordinates):
                    [
                        0.0,
                        slice_info.out_ct_center.min[ax1],
                        slice_info.out_ct_center.max[ax1],
                        size[ax1],
                    ]
                    .map(|x| (x + bounds.min[ax1]) * xform[ax1].y + xform.z.y),
                ];

                // Like `post_xform_crds`, but used for scissor rectangle calculation
                let mut scissor_crds = post_xform_crds.map(|crds| {
                    crds.map(|x| x.round().fmax(0.0).fmin(<u16>::max_value() as f32) as u16)
                });

                // Adjust the endpoints of `scissor_crds` (usually by expanding
                // it by 0–1/2 pixels) so that the scissor rectangles of the
                // generated elements become a partition of `bb`. Without this,
                // the edge antialiasing won't work perfectly.
                for i in 0..2 {
                    let (min, max) = (bb.min[i], bb.max[i]);
                    let valid_slices =
                        (slice_info.valid_slices >> (i ^ axis_swap as usize) * 4) & 0b111;
                    let (start, end) = if xform[i ^ axis_swap as usize][i] >= 0.0 {
                        (min, max)
                    } else {
                        (max, min)
                    };
                    let first_row = [0, 0, 1, 0, 2, 0, 1, 0][valid_slices as usize];
                    let last_row = [0, 0, 1, 1, 2, 2, 2, 2][valid_slices as usize];
                    scissor_crds[i][first_row] = start;
                    scissor_crds[i][last_row + 1] = end;
                }

                // For each slice...
                for (&(s_x, s_y), &uv_xform) in valid_slices.iter().zip(uv_xforms.iter()) {
                    let s = [s_x, s_y];
                    let scissor_x = [scissor_crds[0][s[ax0]], scissor_crds[0][s[ax0] + 1]];
                    let scissor_y = [scissor_crds[1][s[ax1]], scissor_crds[1][s[ax1] + 1]];
                    let scissor = box2! {
                        min: [
                            min(scissor_x[0], scissor_x[1]),
                            min(scissor_y[0], scissor_y[1]),
                        ],
                        max: [
                            max(scissor_x[0], scissor_x[1]),
                            max(scissor_y[0], scissor_y[1]),
                        ],
                    };

                    let scissor = if let Some(x) = scissor.intersection(&bb) {
                        x
                    } else {
                        continue;
                    };

                    let content = Content::from_bmp(bmp.clone(), uv_xform, scissor);

                    elems.push(Elem {
                        flags: elem_flags,
                        opacity,
                        content,
                        scissor,
                        clip_planes: clip_planes.clone(),
                    });
                }
            } // endif slice_by_clip, !use_slicing
        }

        if info.bg_color.a > 0 {
            let mut bg_op = info.bg_color.a as u32;

            if !use_proxy {
                bg_op = (bg_op as f32 * info.opacity) as u32;
            }

            elems.push(Elem {
                flags: if use_proxy {
                    flags![ElemFlags::{}]
                } else {
                    flags![ElemFlags::{CLIP_PLANES | CLIP_PLANES_ANTIALIASED}]
                },
                opacity: bg_op as u16,
                content: Content::Solid(info.bg_color.into()),
                scissor: bb,
                clip_planes: clip_planes.clone(),
            });
        }

        // -------------------------------------------------------------------
        // Generate fragments
        if use_proxy {
            self.open_group(Some(par), info.opacity);
        }

        for elem in elems.into_iter() {
            let elem_i = self.binner.elems.len() as u32;
            let scissor = elem.scissor;

            let sci_min = scissor.min.cast::<usize>().unwrap();
            let sci_max = scissor.max.cast::<usize>().unwrap();
            let bin_xs = sci_min.x / TILE..(sci_max.x + TILE - 1) / TILE;
            let bin_ys = sci_min.y / TILE..(sci_max.y + TILE - 1) / TILE;

            for (bin_x, bin_y) in iproduct!(bin_xs, bin_ys) {
                // TODO: Clip plane cull
                let bin_i = bin_x + bin_y * self.binner.bin_count[0];

                self.prepare_bin(bin_i);

                let frag_i = self.binner.frags.len() as u32;

                let bin = &mut self.binner.bins[bin_i];
                self.binner.frags.push(Frag {
                    elem_i,
                    next_frag_i: bin.frag_first_i,
                    layer: self.layer,
                });

                // Link the new fragment to the front of the fragment list
                bin.frag_first_i = frag_i;
                if bin.frag_last_i == NONE {
                    bin.frag_last_i = frag_i;
                }
            }

            self.binner.elems.push(elem);
        }

        if use_proxy {
            self.close_group();
        }
    }

    /// Prepare the specified bin for adding fragments to a layer `self.layer`.
    fn prepare_bin(&mut self, bin_i: usize) {
        let bin = &mut self.binner.bins[bin_i];

        // TODO: Occlusion culling

        if bin.max_layered_group_i >= self.top_layered_group_i {
            debug_assert_eq!(bin.max_layered_group_i, self.top_layered_group_i);
            return;
        }

        // For one or more layered groups, compositing fragments need to be
        // emitted. Compositing fragments must be inserted *before* any contents
        // because we generate fragment streams in a reverse drawing order.
        while bin.max_layered_group_i < self.top_layered_group_i {
            let group_i = bin.max_layered_group_i as usize;
            let group = &self.groups[group_i];

            if let Some(gr_layer) = &group.layer {
                // Found a layered group. Emit a compositing fragment
                let frag_i = self.binner.frags.len() as u32;

                let dst_layer = gr_layer.layer - 1;

                self.binner.frags.push(Frag {
                    elem_i: gr_layer.elem_i,
                    next_frag_i: bin.frag_first_i,
                    layer: dst_layer,
                });

                // Link the new fragment to the front of the fragment list
                bin.frag_first_i = frag_i;
                if bin.frag_last_i == NONE {
                    bin.frag_last_i = frag_i;
                }

                self.binner.group_bins[group_i].push(bin_i as u32);
            }

            bin.max_layered_group_i += 1;
        }
    }
}

/// Check if the given transformation matrix have the last row `[0 0 1]`.
fn is_affine_xform(xform: Matrix3<f32>) -> bool {
    xform.x.z == 0.0 && xform.y.z == 0.0 && xform.z.z == 1.0
}

/// Transform an axis-aligned box, returning the AABB of the transformed box.
fn xform_aabb(xform: Matrix3<f32>, bx: Box2<f32>) -> Box2<f32> {
    parallelogram_aabb(xform_and_aabb_to_parallelogram(xform, bx))
}

fn parallelogram_aabb(xform: Matrix3<f32>) -> Box2<f32> {
    box2! {
        min: [
            xform.z.x + xform.x.x.fmin(0.0) + xform.y.x.fmin(0.0),
            xform.z.y + xform.x.y.fmin(0.0) + xform.y.y.fmin(0.0),
        ],
        max: [
            xform.z.x + xform.x.x.fmax(0.0) + xform.y.x.fmax(0.0),
            xform.z.y + xform.x.y.fmax(0.0) + xform.y.y.fmax(0.0),
        ],
    }
}

/// Given a transformation matrix `xform` and an axis-aligned box `bx`,
/// construct a matrix representing the parallelogram `P` that is identical to
/// the one produced by transforming `bx` with `xform`.
///
/// Let `M` be the returned matrix. If you transform the square shape
/// represented by  `0 ≤ x, y ≤ 1` using `M`, the output shape is identical to
/// `P`.
#[rustfmt::skip]
fn xform_and_aabb_to_parallelogram(xform: Matrix3<f32>, bx: Box2<f32>) -> Matrix3<f32> {
    let p = xform.transform_point(bx.min);
    let size = bx.max - bx.min;

    Matrix3::new(
        xform.x.x * size.x,     xform.x.y * size.x,     0.0,
        xform.y.x * size.y,     xform.y.y * size.y,     0.0,
        p.x,                    p.y,                    1.0,
    )
}

fn round_aabb_conservative(bx: Box2<f32>) -> Box2<f32> {
    box2! {
        min: [bx.min.x.floor(), bx.min.y.floor()],
        max: [bx.max.x.ceil(), bx.max.y.ceil()],
    }
}

fn saturating_aabb_f32_to_u16(bx: Box2<f32>) -> Option<Box2<u16>> {
    const MAX: f32 = <u16>::max_value() as f32;

    let bx = box2! {
        min: [bx.min.x.fmax(0.0) as u16, bx.min.y.fmax(0.0) as u16],
        max: [bx.max.x.fmin(MAX) as u16, bx.max.y.fmin(MAX) as u16],
    };

    if bx.is_empty() {
        None
    } else {
        Some(bx)
    }
}

/// Given a parallelogram, construct clip planes.
///
/// See `ClipPlanes` as for why we use clip planes.
///
/// `xform` represents an affine tansform that transforms a square shape
/// `0 ≤ x, y ≤ 1` to a parallelogram.
///
///  - The first clip plane `(n, d)` transforms the input regions `x < 0`,
///    `0 ≤ x < 1`, and `1 ≤ x` to `i < d.start`, `d.start ≤ i < d.end`, and
///    `d.end ≤ i`, respectively.
///  - Ditto for the second clip plane, but with `y` instead of `x`.
///
fn xform_to_clip_planes(xform: Matrix3<f32>) -> [ClipPlanes; 2] {
    // The normal vectors of the half planes. Use L1-normalization to ensure
    // the transition width is at least as large as a pixel even when the
    // vectors are not aligned to an axis.
    let n: [Vector2<_>; 2] = [
        [xform.y.y, -xform.y.x].into(),
        [-xform.x.y, xform.x.x].into(),
    ];

    // Use L1-normalization to ensure the transition width is at least as large
    // as a pixel when the vectors are not aligned to an axis.
    let fac = [n[0].x.abs() + n[0].y.abs(), n[1].x.abs() + n[1].y.abs()];
    let mut n = [n[0] / fac[0], n[1] / fac[1]];

    // The widths of the thick straight lines
    let det = xform.x.x * xform.y.y - xform.x.y * xform.y.x;
    let w = [det.abs() / fac[0], det.abs() / fac[1]];

    n[0] *= 1.0f32.copysign(det);
    n[1] *= 1.0f32.copysign(det);

    // The distances from the origin point to the lines
    let p = Vector2::new(xform.z.x, xform.z.y);
    let d = [p.dot(n[0]), p.dot(n[1])];

    // ... and to the clip planes
    let d = [d[0]..d[0] + w[0], d[1]..d[1] + w[1]];

    // Cast to `i32`
    let quant = |x: f32| (x * CLIP_SUB as f32) as i32;
    [
        ClipPlanes {
            n: n[0].map(quant),
            d: quant(d[0].start)..quant(d[0].end),
        },
        ClipPlanes {
            n: n[1].map(quant),
            d: quant(d[1].start)..quant(d[1].end),
        },
    ]
}

/// Check if `ClipPlanes` exactly represent a axis-aligned region aligned to
/// pixel.
fn is_clip_planes_aligned_to_pixel(clip_planes: &ClipPlanes) -> bool {
    let ClipPlanes { n, d } = clip_planes;

    match (n.x.abs(), n.y.abs()) {
        (0, 0) => true,
        (CLIP_SUB, 0) | (0, CLIP_SUB) => d.start % CLIP_SUB == 0 && d.end % CLIP_SUB == 0,
        _ => false,
    }
}

/// Check if `ClipPlanes` exactly represent a axis-aligned region.
fn is_clip_planes_aligned_to_axis(clip_planes: &ClipPlanes) -> bool {
    clip_planes.n.x == 0 || clip_planes.n.y == 0
}

impl<TBmp> Content<TBmp> {
    /// Construct a `Content` from a bitmap image.
    ///
    /// `mat` representes a transformation from render target coordinates to
    /// bitmap (unnormalized UV) coordinates.
    ///
    /// `scissor` is a scissor rectangle specified in the render target space.
    fn from_bmp(bmp: TBmp, mat: Matrix3<f32>, scissor: Box2<u16>) -> Self {
        let quant = |x: f32| (x * UV_SUB as f32) as i32;
        let duv_dx = [quant(mat.x.x), quant(mat.x.y)].into();
        let duv_dy = [quant(mat.y.x), quant(mat.y.y)].into();

        let uv_origin: Point2<f32> =
            mat.transform_point([scissor.min.x as f32 + 0.5, scissor.min.y as f32 + 0.5].into());
        let uv_origin = [quant(uv_origin.x - 0.5), quant(uv_origin.y - 0.5)].into();

        Content::Bmp {
            bmp,
            duv_dx,
            duv_dy,
            uv_origin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use array::*;
    use cgmath::abs_diff_eq;
    use quickcheck::TestResult;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn test_parallelogram_aabb(
        (m00, m01, m02, m10, m11, m12): (f32, f32, f32, f32, f32, f32),
    ) -> bool {
        let mat = Matrix3::new(m00, m10, 0.0, m01, m11, 0.0, m02, m12, 1.0);

        // Calculate the AABB using `parallelogram_aabb`
        let aabb = parallelogram_aabb(mat);

        // Calculate the actual AABB from the corners
        let corners: [Point2<f32>; 4] = [
            [0.0, 0.0].into(),
            [1.0, 0.0].into(),
            [0.0, 1.0].into(),
            [1.0, 1.0].into(),
        ];
        let corners = corners.map(|p| mat.transform_point(p));

        let expected_aabb = box2! {
            min: [
                corners.map(|p| p.x).fmin(),
                corners.map(|p| p.y).fmin(),
            ],
            max: [
                corners.map(|p| p.x).fmax(),
                corners.map(|p| p.y).fmax(),
            ],
        };

        abs_diff_eq!(dbg!(aabb), dbg!(expected_aabb), epsilon = 0.001)
    }

    #[quickcheck]
    fn test_xform_and_aabb_to_parallelogram(
        (m00, m01, m02, m10, m11, m12): (f32, f32, f32, f32, f32, f32),
        bx: Box2<f32>,
    ) -> bool {
        let mat = Matrix3::new(m00, m10, 0.0, m01, m11, 0.0, m02, m12, 1.0);

        let par = xform_and_aabb_to_parallelogram(mat, bx);

        // Calculate the corners using `par`
        let corners: [Point2<f32>; 4] = [
            [0.0, 0.0].into(),
            [1.0, 0.0].into(),
            [0.0, 1.0].into(),
            [1.0, 1.0].into(),
        ];
        let corners = dbg!(corners.map(|p| par.transform_point(p)));

        // Calculate the corners using `mat`
        let actual_corners: [Point2<f32>; 4] = [
            [bx.min.x, bx.min.y].into(),
            [bx.max.x, bx.min.y].into(),
            [bx.min.x, bx.max.y].into(),
            [bx.max.x, bx.max.y].into(),
        ];
        let actual_corners = dbg!(actual_corners.map(|p| mat.transform_point(p)));

        (0..4).all(|i| abs_diff_eq!(corners[i], actual_corners[i], epsilon = 0.1))
    }

    #[test]
    fn test_round_aabb_conservative() {
        assert_eq!(
            round_aabb_conservative(box2! {
                min: [4.0, 6.0],
                max: [42.0, 43.0],
            }),
            box2! {
                min: [4.0, 6.0],
                max: [42.0, 43.0],
            }
        );

        assert_eq!(
            round_aabb_conservative(box2! {
                min: [4.5, 6.9],
                max: [42.3, 43.7],
            }),
            box2! {
                min: [4.0, 6.0],
                max: [43.0, 44.0],
            }
        );
    }

    #[quickcheck]
    fn test_xform_to_clip_planes_aabb(bx: Box2<f32>, transposed: bool) -> TestResult {
        let sz = bx.size();

        // `bx` represents an empty region
        if sz.x == 0.0 || sz.y == 0.0 {
            return TestResult::discard();
        }

        let mat = if !transposed {
            // ┌             ┐
            // │  w   0   x  │
            // │  0   h   y  │
            // │  0   0   1  │
            // └             ┘
            Matrix3::new(sz.x, 0.0, 0.0, 0.0, sz.y, 0.0, bx.min.x, bx.min.y, 1.0)
        } else {
            // ┌             ┐
            // │  0   w   x  │
            // │  h   0   y  │
            // │  0   0   1  │
            // └             ┘
            Matrix3::new(0.0, sz.y, 0.0, sz.x, 0.0, 0.0, bx.min.x, bx.min.y, 1.0)
        };

        let clip_planes = dbg!(xform_to_clip_planes(mat));

        fn eval(cp: &ClipPlanes, p: Point2<f32>) -> i32 {
            cp.n.cast::<f32>().unwrap().dot([p.x, p.y].into()) as i32
        }

        for (s_x, s_y) in iproduct!(0..3, 0..3) {
            let samp: Point2<f32> =
                mat.transform_point([s_x as f32 - 0.5, s_y as f32 - 0.5].into());
            let d_x = eval(&clip_planes[0], samp);
            let d_y = eval(&clip_planes[1], samp);

            dbg!((s_x, s_y, samp, d_x, d_y));

            let ok_x = match s_x {
                0 => d_x <= clip_planes[0].d.start,
                1 => clip_planes[0].d.contains(&d_x),
                2 => clip_planes[0].d.end <= d_x,
                _ => unreachable!(),
            };
            let ok_y = match s_y {
                0 => d_y <= clip_planes[1].d.start,
                1 => clip_planes[1].d.contains(&d_y),
                2 => clip_planes[1].d.end <= d_y,
                _ => unreachable!(),
            };

            if !ok_x || !ok_y {
                return TestResult::failed();
            }
        }

        TestResult::passed()
    }

    #[quickcheck]
    fn clip_planes_aligned_to_pixel(bx: Box2<i8>, transposed: bool) -> TestResult {
        // `bx` is aligned to pixels becaue of the use of `i8`
        let bx = bx.cast::<f32>().unwrap();
        let sz = bx.size();
        if sz.x == 0.0 || sz.y == 0.0 {
            return TestResult::discard();
        }

        let mat = if !transposed {
            Matrix3::new(sz.x, 0.0, 0.0, 0.0, sz.y, 0.0, bx.min.x, bx.min.y, 1.0)
        } else {
            Matrix3::new(0.0, sz.y, 0.0, sz.x, 0.0, 0.0, bx.min.x, bx.min.y, 1.0)
        };

        let clip_planes = dbg!(xform_to_clip_planes(mat));
        let ok = clip_planes.iter().all(is_clip_planes_aligned_to_pixel);
        TestResult::from_bool(ok)
    }

    #[quickcheck]
    fn clip_planes_aligned_to_axis(bx: Box2<f32>, transposed: bool) -> TestResult {
        let sz = bx.size();
        if sz.x == 0.0 || sz.y == 0.0 {
            return TestResult::discard();
        }

        let mat = if !transposed {
            Matrix3::new(sz.x, 0.0, 0.0, 0.0, sz.y, 0.0, bx.min.x, bx.min.y, 1.0)
        } else {
            Matrix3::new(0.0, sz.y, 0.0, sz.x, 0.0, 0.0, bx.min.x, bx.min.y, 1.0)
        };

        let clip_planes = dbg!(xform_to_clip_planes(mat));
        let ok = clip_planes.iter().all(is_clip_planes_aligned_to_axis);
        TestResult::from_bool(ok)
    }

    #[test]
    fn clip_planes_not_aligned_to_pixel() {
        let mat = Matrix3::new(30.5, 0.0, 0.0, 0.0, 10.5, 0.0, 5.0, 5.0, 1.0);
        let clip_planes = dbg!(xform_to_clip_planes(mat));
        assert!(!clip_planes.iter().any(is_clip_planes_aligned_to_pixel));
    }

    #[test]
    fn clip_planes_not_aligned_to_axis() {
        let mat = Matrix3::new(30.5, 0.5, 0.0, 15.0, 10.5, 0.0, 5.0, 5.0, 1.0);
        let clip_planes = dbg!(xform_to_clip_planes(mat));
        assert!(!clip_planes.iter().any(is_clip_planes_aligned_to_axis));
    }

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

    #[test]
    fn draw() {
        let mut binner = Binner::new();

        let mats = [
            // pixel-aligned
            Matrix3::new(5.0, 0.0, 0.0, 0.0, 7.0, 0.0, 10.0, 15.0, 1.0),
            // axis-aligned
            Matrix3::new(5.5, 0.0, 0.0, 0.0, 7.3, 0.0, 10.1, 15.0, 1.0),
            // generic
            Matrix3::new(5.5, 7.5, 0.0, 6.0, 4.5, 0.0, 10.0, 15.0, 1.0),
        ];

        let group_types = [
            (None, 1.0),
            (None, 0.5),
            (Some(mats[0]), 1.0),
            (Some(mats[1]), 1.0),
            (Some(mats[2]), 1.0),
            (Some(mats[0]), 0.5),
            (Some(mats[1]), 0.5),
            (Some(mats[2]), 0.5),
        ];

        let ct_centers = [
            box2! { min: [0.0, 0.0], max: [1.0, 1.0] },
            box2! { min: [1.0, 0.0], max: [1.0, 1.0] },
            box2! { min: [0.2, 0.3], max: [0.7, 0.8] },
            box2! { min: [0.0, 0.3], max: [0.7, 0.8] },
        ];

        let bg_colors = [
            RGBA8::new(40, 60, 80, 0),
            RGBA8::new(40, 60, 80, 200),
            RGBA8::new(40, 60, 80, 255),
        ];

        let ops = [0.0, 0.6, 1.0];

        for (&xform, &(gr_xform, gr_op), &ct_center, &bg_color, &op) in iproduct!(
            mats.iter(),
            group_types.iter(),
            ct_centers.iter(),
            bg_colors.iter(),
            ops.iter()
        ) {
            dbg!((xform, gr_xform, gr_op, ct_center));
            let mut builder = binner.build([200, 100]);
            builder.open_group(gr_xform, gr_op);

            builder.push_elem(ElemInfo {
                xform,
                bounds: box2! { min: [0.0, 0.0], max: [20.0, 20.0] },
                contents_center: ct_center,
                contents_scale: 1.0,
                bitmap: Some(TestBmp),
                bg_color,
                opacity: op,
            });

            builder.close_group();
            builder.finish();
        }
    }
}
