//! Binner (not "bin"ary!)
use arrayvec::ArrayVec;
use bitflags::bitflags;
use cggeom::{box2, Box2};
use cgmath::{Matrix3, Vector2};
use std::ops::Range;

use super::NUM_LAYERS;

/// A temporary storage for binning.
#[derive(Debug)]
pub struct Binner<TBmp> {
    bins: Vec<Bin>,
    elems: Vec<Elem<TBmp>>,
    frag_pool: Vec<Frag>,
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
    frag_first: u32,
    /// The last fragment in the bin's fragment list, or `NONE`.
    frag_last: u32,
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

    /// Clip planes (enabled by `ElemFlags::CLIP_PLANES`). Given window
    /// pixel coordinates `p = vec2(x, y)`, the pixel is considered included if
    /// `(0..2).all(|i| clip_dists[i].contains(&p.dot(clip_planes[i])))`.
    /// The lengths of `clip_planes` must be close to `CLIP_SUB` for edge
    /// antialiasing.
    clip_planes: [Vector2<i32>; 2],
    clip_dists: [Range<i32>; 2],
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
    /// fragment, respectively.
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
    flags: FragFlags,

    /// Index of `Elem`.
    elem_id: u32,

    /// The next index of `Frag`. `NONE` if there is none.
    next_frag_id: u32,

    /// The 0-based layer number. Layers are like CPU registers and can be used
    /// to implement complex composite operations. Layer 0 is used as output.
    /// The valid range is `0..NUM_LAYERS`.
    layer: u8,
}

const NONE: u32 = 0xffffffff;

bitflags! {
    struct FragFlags: u8 {
        /// Instructs to clear the destination layer, instead of drawing
        /// something on it. `elem_id` is ignored.
        const CLEAR = 1 << 0;
    }
}

impl<TBmp: Bmp> Binner<TBmp> {
    /// Construct a `Binner`.
    pub fn new() -> Self {
        unimplemented!()
    }

    /// Initialize the storage to accomodate the specified render target size,
    /// and start filling bins.
    pub(super) fn build(&mut self, size: [usize; 2]) -> BinnerBuilder<'_, TBmp> {
        assert!(size[0] <= <u16>::max_value() as usize);
        assert!(size[1] <= <u16>::max_value() as usize);

        // TODO
        BinnerBuilder {
            binner: self,
            groups: ArrayVec::new(),
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
    pub transform: Matrix3<f32>,
    pub bounds: Box2<f32>,
    pub contents_center: Box2<f32>,
    pub contents_scale: f32,
    pub bitmap: Option<TBmp>,
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
    groups: ArrayVec<[BuilderGroup; NUM_LAYERS - 1]>,

    /// The current layer. It starts at `0`.
    ///
    /// It's equal to: `groups.iter().map(|g| g.elem_id.is_some() as usize).sum()`.
    layer: usize,

    /// The current scissor rectangle. Initially, this represents the entirety
    /// of the render target. `None` means empty.
    scissor: Option<Box2<u16>>,
}

#[derive(Debug)]
struct BuilderGroup {
    /// Points `Elem` that is used to composite this group onto the parent
    /// group. `None` if this group does not have its own layer.
    ///
    /// The reason we store this is that we want to defer the generation of
    /// compositing fragments so that we can skip it for bins don't include
    /// the group's content. This is especially important when `mask_transform`
    /// is `None` because, in this case, the group encompasses entire the
    /// render target even if its content occupies a small portion.
    elem_id: Option<u32>,

    /// The old scissor rectangle to be restored when this group is closed.
    old_scissor: Option<Box2<u16>>,
}

impl<TBmp: Bmp> BinnerBuilder<'_, TBmp> {
    pub(super) fn finish(self) {
        assert!(self.groups.is_empty(), "All non-root groups must be closed");
        assert_eq!(self.layer, 0);
        unimplemented!()
    }

    /// Open a composition group.
    ///
    /// `mask_transform` describes the mask shape of the group. A square shape
    /// `0 ≤ x, y ≤ 1` is transformed using this transformation matrix. The last
    /// row of the matrix must be `[0 0 1]`. The resulting parallelogram is
    /// used as the mask shape. Masking is not applied if it's `None`.
    pub(super) fn open_group(&mut self, mask_transform: Option<Matrix3<f32>>, opacity: f32) {
        unimplemented!();

        // - If the new scissor rectangle is empty, the group doesn't need a
        //   layer. Otherwise:
        // - If `opacity` is less than `1`, the group needs a layer.
        // - If the mask shape isn't an axis-aligned box that is perfectly
        //   aligned to pixels, the group needs a layer. Also, the compositing
        //   `Elem` should use clip planes for masking.

        self.groups.push(BuilderGroup {
            elem_id: None, // TODO
            old_scissor: self.scissor,
        });
    }

    /// Close a composition group and insert it to the parent group.
    pub(super) fn close_group(&mut self) {
        let _ = self.groups.pop().expect("Cannot close the root group");
        // Note: `self.layer` can be `0` if the group doesn't generate a layer

        unimplemented!();
    }

    /// Insert an element.
    pub(super) fn push_elem(&mut self, info: ElemInfo<TBmp>) {
        unimplemented!()
    }
}
