//! Provides a retained-mode rendering API that closely follows
//! the layering API provided by `tcw3::pal::iface::Wm`. However, there are
//! some differences:
//!
//!  - `update_wnd` only returns a dirty region that was updated since the
//!    last update. The client must supply an image buffer and call `render_wnd`
//!    to render the window contents.
//!
//!  - This module do not use most of `WndAttrs`'s fields, so provides a
//!    different API for setting window attributes.
//!
use alt_fp::FloatOrd;
use bitflags::bitflags;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{prelude::*, Matrix3, Vector2};
use iterpool::{Pool, PoolPtr};

use super::super::iface;

use super::{
    binner::{
        round_aabb_conservative, xform_aabb, xform_and_aabb_to_parallelogram, Binner,
        BinnerBuilder, Bmp, ElemInfo,
    },
    rast::rasterize,
    utils::Box2UsizeUnion,
};

/// The window handle type of [`Screen`].
#[derive(Debug, Clone)]
pub struct HWnd {
    ptr: PoolPtr,
}

/// The layer handle type of [`Screen`].
#[derive(Debug, Clone, PartialEq)]
pub struct HLayer {
    ptr: PoolPtr,
}

/// Manages layers and windows.
#[derive(Debug)]
pub struct Screen<TBmp> {
    layers: Pool<Layer<TBmp>>,
    wnds: Pool<Wnd>,
}

#[derive(Debug)]
struct Layer<TBmp> {
    /// Possible references include: `Layer::sublayers`, `Layer::new_sublayers`,
    /// and a client-visible `HLayer`.
    ref_count: u8,

    dirty: LayerDirtyFlags,
    attrs: LayerAttrs<TBmp>,
    sublayers: Vec<HLayer>,

    new_sublayers: Option<Vec<HLayer>>,

    /// This value is based on the uncommitted state
    /// (`sublayers.unwrap_or(new_sublayers)`).
    superlayer: Option<Superlayer>,

    // --- The following fields are derived values calculated during an update ---
    /// The bounding box.
    bbox: Option<Box2<usize>>,

    /// The bounding box of the layer content (not sublayers).
    bbox_content: Option<Box2<usize>>,

    /// The bounding box of the sublayers.
    bbox_sublayers: Option<Box2<usize>>,

    /// The bounding box of the clipping mask for the sublayers.
    /// Valid only if `MASK_TO_BOUNDS` is set.
    bbox_mask: Option<Box2<usize>>,

    // ----------- The following fields are only used during an update -----------
    sublayers_i: usize,
    dirty_rect: Option<Box2<usize>>,
    old_bbox: Option<Box2<usize>>,
}

const NONE: usize = usize::max_value();

#[derive(Debug, Clone, PartialEq)]
enum Superlayer {
    Layer(HLayer),
    Wnd,
}

bitflags! {
    struct LayerDirtyFlags: u8 {
        /// The layer content has uncommited changes.
        const CONTENT = 1 << 0;

        /// The layer opacity has uncommited changes. This affects the layer
        /// content as well as the sublayers.
        const OPACITY = 1 << 1;

        /// One or more sublayers may have uncommited changes in the layer
        /// contents (`LayerDirtyFlags::CONTENT`), opacity (`LayerDirtyFlags::OPACITY`)
        /// and/or sublayers (`Layer::new_sublayers`).
        const DESCENDANT = 1 << 2;
    }
}

#[derive(Debug)]
struct LayerAttrs<TBmp> {
    transform: Matrix3<f32>,
    contents: Option<TBmp>,
    bounds: Box2<f32>,
    contents_center: Box2<f32>,
    contents_scale: f32,
    bg_color: iface::RGBAF32,
    opacity: f32,
    flags: iface::LayerFlags,
}

impl<TBmp> Default for LayerAttrs<TBmp> {
    fn default() -> Self {
        Self {
            transform: Matrix3::identity(),
            contents: None,
            bounds: Box2::zero(),
            contents_center: box2! { min: [0.0; 2], max: [1.0; 2] },
            contents_scale: 1.0,
            bg_color: [0.0; 4].into(),
            opacity: 1.0,
            flags: iface::LayerFlags::empty(),
        }
    }
}

impl<TBmp> LayerAttrs<TBmp> {
    fn assign<TLayer>(&mut self, attrs: iface::LayerAttrs<TBmp, TLayer>) {
        if let Some(x) = attrs.transform {
            self.transform = x;
        }
        if let Some(x) = attrs.contents {
            self.contents = x;
        }
        if let Some(x) = attrs.bounds {
            self.bounds = x;
        }
        if let Some(x) = attrs.contents_center {
            self.contents_center = x;
        }
        if let Some(x) = attrs.contents_scale {
            self.contents_scale = x;
        }
        if let Some(x) = attrs.bg_color {
            self.bg_color = x;
        }
        if let Some(x) = attrs.opacity {
            self.opacity = x;
        }
        if let Some(x) = attrs.flags {
            self.flags = x;
        }
    }
}

#[derive(Debug)]
struct Wnd {
    /// `true` if an entire window needs to be updated. This does not
    /// reflect the root view's dirty flag.
    dirty: bool,
    size: [usize; 2],
    dpi_scale: f32,
    root: Option<HLayer>,
}

impl<TBmp: Bmp> Screen<TBmp> {
    pub fn new() -> Self {
        Self {
            layers: Pool::new(),
            wnds: Pool::new(),
        }
    }

    pub fn new_wnd(&mut self) -> HWnd {
        let ptr = self.wnds.allocate(Wnd {
            dirty: true,
            size: [0; 2],
            dpi_scale: 1.0,
            root: None,
        });

        HWnd { ptr }
    }

    pub fn remove_wnd(&mut self, hwnd: &HWnd) {
        self.set_wnd_layer(hwnd, None);
        self.wnds.deallocate(hwnd.ptr);
    }

    pub fn set_wnd_size(&mut self, wnd: &HWnd, size: [usize; 2]) {
        let wnd = &mut self.wnds[wnd.ptr];
        wnd.size = size;
        wnd.dirty = true;
    }

    pub fn set_wnd_dpi_scale(&mut self, wnd: &HWnd, dpi_scale: f32) {
        let wnd = &mut self.wnds[wnd.ptr];
        wnd.dpi_scale = dpi_scale;
        wnd.dirty = true;
    }

    pub fn set_wnd_layer(&mut self, hwnd: &HWnd, hlayer: Option<HLayer>) {
        let wnd = &mut self.wnds[hwnd.ptr];
        wnd.dirty = true;

        // Detach the old root
        if let Some(hlayer) = wnd.root.take() {
            let layer = &mut self.layers[hlayer.ptr];
            debug_assert_eq!(layer.superlayer, Some(Superlayer::Wnd));
            layer.superlayer = None;

            self.release_layer(&hlayer);
        }

        // Attach the new root
        if let Some(hlayer) = &hlayer {
            let layer = &mut self.layers[hlayer.ptr];
            debug_assert_eq!(
                layer.superlayer, None,
                "layers only can have up to one parent"
            );
            layer.superlayer = Some(Superlayer::Wnd);
            layer.ref_count += 1;
        }

        let wnd = &mut self.wnds[hwnd.ptr];
        wnd.root = hlayer;
    }

    pub fn new_layer(&mut self, attrs: iface::LayerAttrs<TBmp, HLayer>) -> HLayer {
        let layer = Layer {
            ref_count: 1,
            dirty: LayerDirtyFlags::CONTENT,
            attrs: LayerAttrs::default(),
            superlayer: None,
            bbox: None,
            bbox_content: None,
            bbox_sublayers: None,
            bbox_mask: None,
            old_bbox: None,
            new_sublayers: None,
            sublayers: Vec::new(),
            sublayers_i: NONE,
            dirty_rect: None,
        };

        let ptr = self.layers.allocate(layer);
        let hlayer = HLayer { ptr };

        self.set_layer_attr(&hlayer, attrs);

        hlayer
    }

    pub fn set_layer_attr(&mut self, layer: &HLayer, mut attrs: iface::LayerAttrs<TBmp, HLayer>) {
        let mut descendant_dirty = false;

        if let Some(new_new_sublayers) = &attrs.sublayers {
            let old_new_sublayers = self.layers[layer.ptr].new_sublayers.take();

            // If `attrs.sublayers` is set and `layer.new_sublayers` is already set,
            // detach `layer.new_sublayers` first
            if let Some(layers) = old_new_sublayers {
                for hlayer in layers {
                    let sublayer = &mut self.layers[hlayer.ptr];
                    debug_assert!(sublayer.superlayer == Some(Superlayer::Layer(layer.clone())));

                    self.release_layer(&hlayer);
                }
            }

            // Link sublayers
            for hlayer in new_new_sublayers.iter() {
                let sublayer = &mut self.layers[hlayer.ptr];
                debug_assert!(
                    sublayer.superlayer.is_none(),
                    "layers only can have up to one parent"
                );
                sublayer.superlayer = Some(Superlayer::Layer(layer.clone()));
                sublayer.ref_count += 1;

                let dirty = !sublayer.dirty.is_empty();
                descendant_dirty |= dirty | sublayer.new_sublayers.is_some();
            }

            // Assign `new_sublayers` later when we mutably borrow `self.layers[layer.ptr]`
        }

        let layer = &mut self.layers[layer.ptr];

        let sublayers_modified = attrs.sublayers.is_some();

        if let Some(new_new_sublayers) = attrs.sublayers.take() {
            assert!(layer.new_sublayers.is_none());
            layer.new_sublayers = Some(new_new_sublayers);
        }

        let content_modified = attrs.transform.is_some()
            | attrs.contents.is_some()
            | attrs.bounds.is_some()
            | attrs.contents_center.is_some()
            | attrs.contents_scale.is_some()
            | attrs.bg_color.is_some()
            | attrs.opacity.is_some()
            | attrs.flags.is_some();

        let opacity_modified = attrs.opacity.is_some();

        if content_modified {
            layer.attrs.assign(attrs);
        }

        // Update dirty flags
        if content_modified {
            layer.dirty |= LayerDirtyFlags::CONTENT;
        }
        if opacity_modified {
            layer.dirty |= LayerDirtyFlags::OPACITY;
        }
        if descendant_dirty {
            layer.dirty |= LayerDirtyFlags::DESCENDANT;
        }

        if !(content_modified | sublayers_modified | descendant_dirty) {
            return;
        }

        let mut superlayer = layer.superlayer.clone();

        while let Some(Superlayer::Layer(hlayer)) = superlayer {
            let layer = &mut self.layers[hlayer.ptr];
            superlayer = layer.superlayer.clone();

            if layer.dirty.intersects(LayerDirtyFlags::DESCENDANT) {
                break;
            }
            layer.dirty |= LayerDirtyFlags::DESCENDANT;
        }
    }

    pub fn remove_layer(&mut self, layer: &HLayer) {
        self.release_layer(layer);
    }

    fn release_layer(&mut self, layer: &HLayer) {
        {
            let layer = &mut self.layers[layer.ptr];
            layer.ref_count -= 1;
            if layer.ref_count > 0 {
                return;
            }
        }

        let layer = self.layers.deallocate(layer.ptr).unwrap();

        for sublayer in layer.sublayers {
            self.release_layer(&sublayer);
        }

        if let Some(sublayers) = layer.new_sublayers {
            for sublayer in sublayers {
                self.release_layer(&sublayer);
            }
        }
    }

    /// Calculate the portion of a window which has been updated since the last
    /// time `update_wnd` was called.
    pub fn update_wnd(&mut self, hwnd: &HWnd) -> Option<Box2<usize>> {
        let wnd = &mut self.wnds[hwnd.ptr];
        let root = wnd.root.clone();
        let ctx = UpdateCtx {
            wnd_size_f32: [wnd.size[0] as f32, wnd.size[1] as f32],
            dpi_scale: wnd.dpi_scale,
            full_update: wnd.dirty,
        };

        let mut dirty_region = None;

        if let Some(hlayer) = root {
            self.update_layer(&hlayer, &ctx);

            let layer = &self.layers[hlayer.ptr];
            dirty_region = layer.dirty_rect;
        }

        let wnd = &mut self.wnds[hwnd.ptr];
        if wnd.dirty {
            wnd.dirty = false;
            dirty_region = Some(box2! { min: [0, 0], max: wnd.size });
        }

        dirty_region
    }

    /// Clear the dirty flag of a layer, updating fields including: `dirty_rect`,
    /// `bbox`, `bbox_content`, `bbox_sublayers`, and `bbox_clip`.
    fn update_layer(&mut self, hlayer: &HLayer, ctx: &UpdateCtx) {
        let layer = &mut self.layers[hlayer.ptr];

        let should_check_sublayers = layer.dirty.contains(LayerDirtyFlags::DESCENDANT)
            | layer.new_sublayers.is_some()
            | ctx.full_update;
        let sublayers_change;

        // Update `sublayers`, saving the old value as `old_sublayers`
        let old_sublayers = if let Some(new_sublayers) = layer.new_sublayers.take() {
            Some(std::mem::replace(&mut layer.sublayers, new_sublayers))
        } else {
            None
        };

        if should_check_sublayers {
            // Save the old `bbox` before it's updated by `update_layer`
            //
            // For removed sublayers, the old `bbox` is needed to calculate the
            // dirty region. But `update_layer` is called only for the existing
            // sublayers. Then why do we have to preserve the old `bbox`?
            //
            // The reason is that the diff algorithm may falsely report removed
            // elements when some of the elements are reordered. Consider the
            // lists `[1, 2, 3]` and `[1, 3, 2]`. The algorithm produces output
            // like this: `[Remove(2), Add(3), Add(2), Remove(3)]`.
            if let (Some(old_sublayers), false) = (&old_sublayers, ctx.full_update) {
                for hlayer in old_sublayers.iter() {
                    let layer = &mut self.layers[hlayer.ptr];
                    layer.old_bbox = layer.bbox;
                }
            }

            // Temporarily move out `sublayers` to check the sublayers
            let sublayers = std::mem::replace(&mut self.layers[hlayer.ptr].sublayers, Vec::new());

            for hlayer in sublayers.iter() {
                self.update_layer(&hlayer, ctx);
            }

            let mut uni_dirty_rect = Box2UsizeUnion::new();

            // Do not utilize `old_sublayers` if a full update is requested
            if let (Some(old_sublayers), false) = (&old_sublayers, ctx.full_update) {
                // Sublayers might have been added and/or removed. This is the linear-time
                // implementation of the algorithm explained in the design document.
                for (i, hlayer) in old_sublayers.iter().enumerate() {
                    debug_assert_eq!(self.layers[hlayer.ptr].sublayers_i, NONE);
                    self.layers[hlayer.ptr].sublayers_i = i;
                }

                // The optimization in a `if` statement below would break if
                // the indices do not fit in `isize`
                assert!(old_sublayers.len() < isize::max_value() as usize);

                let mut cursor = 0;

                for hlayer in sublayers.iter() {
                    let layer = &self.layers[hlayer.ptr];

                    // (if layer.sublayers_i < cursor || layer.sublayers_i == NONE)
                    if (layer.sublayers_i as isize) < (cursor as isize) {
                        // `layer` was inserted
                        uni_dirty_rect.insert(layer.bbox);
                    } else {
                        // `old_sublayers[cursor..layer.sublayers_i]` was removed.
                        // `layer` was moved from `old_sublayers[layer.sublayers_i]`.
                        for hlayer2 in old_sublayers[cursor..layer.sublayers_i].iter() {
                            uni_dirty_rect.insert(self.layers[hlayer2.ptr].old_bbox);
                        }
                        cursor = layer.sublayers_i + 1;
                        uni_dirty_rect.insert(layer.dirty_rect);
                    }
                }

                // `old_sublayers[cursor..]` was removed.
                for hlayer2 in old_sublayers[cursor..].iter() {
                    uni_dirty_rect.insert(self.layers[hlayer2.ptr].old_bbox);
                }

                for hlayer in old_sublayers.iter() {
                    debug_assert_ne!(self.layers[hlayer.ptr].sublayers_i, NONE);
                    self.layers[hlayer.ptr].sublayers_i = NONE;
                }
            } else {
                // Sublayers did not change. Just compute the sum of their
                // dirty regions
                for hlayer in sublayers.iter() {
                    let layer = &self.layers[hlayer.ptr];
                    uni_dirty_rect.insert(layer.dirty_rect);
                }
            }

            // `bbox_sublayers` changes only if something changes with sublayers.
            // It's okay to assume it doesn't change if `uni_dirty_rect` is `None`.
            if uni_dirty_rect.into_box2().is_some() || ctx.full_update {
                let uni_bbox: Box2UsizeUnion = sublayers
                    .iter()
                    .map(|hlayer| self.layers[hlayer.ptr].bbox)
                    .collect();

                sublayers_change = Some((uni_bbox.into_box2(), uni_dirty_rect.into_box2()));
            } else {
                sublayers_change = None;
            };

            // Return `sublayers`
            self.layers[hlayer.ptr].sublayers = sublayers;

            // Unlink `old_sublayers`
            if let Some(old_sublayers) = old_sublayers {
                for hlayer in old_sublayers {
                    self.release_layer(&hlayer);
                }
            }
        } else {
            sublayers_change = None;
        }

        // Borrow again
        let layer = &mut self.layers[hlayer.ptr];

        // The final color value of a layer is calculated like this:
        //
        //    cₚ = AlphaOver(c_content, c_sublayers * m_mask)
        //          (if MASK_TO_BOUNDS is enabled)
        //    cₚ = AlphaOver(c_content, c_sublayers)
        //          (if MASK_TO_BOUNDS is disabled)
        //    cₒ = cₚ * m_opacity
        //
        // `should_check_content` indicates changes in `c_content`,
        // and `m_mask`. `sublayers_change` indicates changes in `c_sublayers`.
        //
        // We define an arithmetic system to express dirty regions. For example,
        // `c_context'` represents the set of pixels possibly affected by
        // the changes we are making on `c_context`. In this system, blending
        // operations can be defined as follows:
        //
        //     (a * b)' = ab' + a'b + a'b'      (masking)
        //     AlphaOver(a, b)' = a' + b'       (alpha over)
        //
        // Based on this system:
        //
        //    cₚ' = c_content' + c_sublayers' * m_mask
        //           + c_sublayers * m_mask' + c_sublayers' * m_mask'
        //          (if MASK_TO_BOUNDS is enabled)
        //    cₚ' = c_content' + c_sublayers'
        //          (if MASK_TO_BOUNDS is disabled)
        //    cₒ' = cₚ * m_opacity' + cₚ' * m_opacity
        //
        // We assume the layer opacity (`m_opacity`) is not zero.
        //
        // Thus, the final dirty rect is:
        //
        //    dirty_rect
        //         = dirty_content + dirty_sublayers * bbox_mask
        //           + bbox_sublayers * dirty_mask + dirty_sublayers * dirty_mask
        //           + dirty_opacity * (bbox_content + bbox_sublayers * bbox_mask)
        //          (if MASK_TO_BOUNDS is enabled)
        //    dirty_rect
        //         = dirty_content + dirty_sublayers * opacity
        //           + dirty_opacity * (bbox_content + bbox_sublayers)
        //          (if MASK_TO_BOUNDS is disabled)
        //
        // This `uni_dirty_rect` is used for summation of the RHS of these formulae.
        let mut uni_dirty_rect = Box2UsizeUnion::new();
        let (dirty_sublayers, dirty_mask);

        if let Some((bbox, dirty_rect)) = sublayers_change {
            layer.bbox_sublayers = bbox;
            dirty_sublayers = dirty_rect;
        } else {
            dirty_sublayers = None;
        }

        let should_check_content = layer.dirty.contains(LayerDirtyFlags::CONTENT) | ctx.full_update;

        if should_check_content {
            let tx = scale_mat3(layer.attrs.transform, ctx.dpi_scale);
            let bx = xform_aabb(tx, layer.attrs.bounds);
            let bx = round_aabb_conservative(bx);
            let size = ctx.wnd_size_f32;
            let bx = box2! {
                min: [bx.min.x.fmax(0.0) as usize, bx.min.y.fmax(0.0) as usize],
                max: [bx.max.x.fmin(size[0]) as usize, bx.max.y.fmin(size[1]) as usize],
            };
            let bx = if bx.is_empty() { None } else { Some(bx) };

            // Does this layer has a content? But even if it doesn't, `bx` is
            // used for sublayer masking.
            let has_content = layer.attrs.contents.is_some() || layer.attrs.bg_color.a > 0.0;

            let new_bbox_content = bx.filter(|_| has_content);
            let new_bbox_mask = bx;

            let dirty_content = bbox2_union(new_bbox_content, layer.bbox_content);
            dirty_mask = bbox2_union(new_bbox_mask, layer.bbox_mask);

            layer.bbox_content = new_bbox_content;
            layer.bbox_mask = new_bbox_mask;

            uni_dirty_rect.insert(dirty_content);
        } else {
            dirty_mask = None;
        }

        let mask_to_bounds = (layer.attrs.flags).contains(iface::LayerFlags::MASK_TO_BOUNDS);

        if mask_to_bounds {
            uni_dirty_rect.insert(bbox2_intersect(dirty_sublayers, layer.bbox_mask));
            uni_dirty_rect.insert(bbox2_intersect(layer.bbox_mask, dirty_sublayers));
            uni_dirty_rect.insert(bbox2_intersect(dirty_mask, dirty_sublayers));
        } else {
            uni_dirty_rect.insert(dirty_sublayers);
        }

        let dirty_opacity = layer.dirty.contains(LayerDirtyFlags::OPACITY);

        if dirty_opacity {
            uni_dirty_rect.insert(layer.bbox_content);
            if mask_to_bounds {
                uni_dirty_rect.insert(bbox2_intersect(layer.bbox_mask, layer.bbox_sublayers));
            } else {
                uni_dirty_rect.insert(layer.bbox_sublayers);
            }
        }

        layer.dirty_rect = uni_dirty_rect.into_box2();
        layer.dirty = LayerDirtyFlags::empty();

        // Recalculate `bbox`
        if should_check_content | should_check_sublayers {
            layer.bbox = {
                let mut uni = Box2UsizeUnion::new();
                uni.insert(layer.bbox_content);
                if mask_to_bounds {
                    uni.insert(bbox2_intersect(layer.bbox_mask, layer.bbox_sublayers));
                } else {
                    uni.insert(layer.bbox_sublayers);
                }
                uni.into_box2()
            };
        }
    }
}

struct UpdateCtx {
    wnd_size_f32: [f32; 2],

    dpi_scale: f32,

    /// Instructs to ignore the dirty flags of layers. All derived fields in
    /// `Layer` will be recalculated.
    ///
    /// The result of the dirty region calculation done in `update_layer` is
    /// disregarded when this flag if set. I could have added another version of
    /// `update_layer` for the case when this flag is set. I did not because:
    /// (1) It's not a hot code path. (2) That would probably increase the code
    /// size.
    full_update: bool,
}

impl<TBmp: Bmp> Screen<TBmp> {
    /// Render the content of a window to the specified image buffer.
    /// The rendered area is limited to `rect`.
    ///
    /// `rect` is usually based on the return value of `update_wnd`, but does
    /// not necessarily have to be to meet specific requirements (such as
    /// double buffering).
    ///
    /// Let `size` be `bx.size()`.  `out.len()` must be at least
    /// `out_stride * (size[1] - 1) + size[0] * 4`.
    ///
    /// `binner` is used as a temporary storage.
    pub fn render_wnd(
        &mut self,
        hwnd: &HWnd,
        out: &mut [u8],
        out_stride: usize,
        bx: Box2<usize>,
        binner: &mut Binner<TBmp>,
    ) {
        let wnd = &self.wnds[hwnd.ptr];
        assert!(bx.max.x <= wnd.size[0] && bx.max.y <= wnd.size[1]);
        assert!(bx.is_valid());

        let mut builder = binner.build(bx.size().into());
        if let Some(root) = &wnd.root {
            let ctx = RenderCtx {
                dpi_scale: wnd.dpi_scale,
                offset: [bx.min.x as f32, bx.min.y as f32].into(),
            };
            self.binner_build_layer(&mut builder, &ctx, &self.layers[root.ptr]);
        }
        builder.finish();

        rasterize(&binner, out, out_stride);
    }

    fn binner_build_layer(
        &self,
        builder: &mut BinnerBuilder<'_, TBmp>,
        ctx: &RenderCtx,
        layer: &Layer<TBmp>,
    ) {
        // If the layer has both of a content and sublayers, and it's translucent,
        // then we have to create an outer group for group opacity effect.
        // TODO: Actually, `push_elem` creates an implicit group under a variety of
        //       situations. This could be avoided if `layer` has `MASK_TO_BOUNDS`,
        //       i.e., sublayers are masked by this layer's bounds.
        let attrs = &layer.attrs;
        let has_sublayers = layer.sublayers.len() > 0;
        let has_content = attrs.bg_color.a > 0.0 || attrs.contents.is_some();

        let use_opacity_group = has_sublayers && has_content && attrs.opacity < 1.0;

        let inner_opacity = if use_opacity_group {
            1.0
        } else {
            attrs.opacity
        };

        if use_opacity_group {
            builder.open_group(None, attrs.opacity);
        }

        let transform = scale_mat3(attrs.transform, ctx.dpi_scale);
        let transform = translate_neg_mat3(transform, ctx.offset);

        if has_sublayers {
            let mask_xform = if (attrs.flags).contains(iface::LayerFlags::MASK_TO_BOUNDS) {
                Some(xform_and_aabb_to_parallelogram(transform, attrs.bounds))
            } else {
                None
            };
            builder.open_group(mask_xform, inner_opacity);

            for hlayer in layer.sublayers.iter().rev() {
                self.binner_build_layer(builder, ctx, &self.layers[hlayer.ptr]);
            }

            builder.close_group();
        }

        if has_content {
            let bg_color = attrs.bg_color;
            let to_u8 = |x: f32| (x.fmax(0.0).fmin(1.0) * 255.0 + 0.5) as u8;

            builder.push_elem(ElemInfo {
                xform: transform,
                bounds: attrs.bounds,
                contents_center: attrs.contents_center,
                contents_scale: attrs.contents_scale,
                bitmap: attrs.contents.clone(),
                bg_color: [
                    to_u8(bg_color.b),
                    to_u8(bg_color.g),
                    to_u8(bg_color.r),
                    to_u8(bg_color.a),
                ]
                .into(),
                opacity: inner_opacity,
            });
        }

        if use_opacity_group {
            builder.close_group();
        }
    }
}

struct RenderCtx {
    dpi_scale: f32,
    offset: Vector2<f32>,
}

fn bbox2_intersect(x: Option<Box2<usize>>, y: Option<Box2<usize>>) -> Option<Box2<usize>> {
    match (x, y) {
        (Some(x), Some(y)) => x.intersection(&y),
        _ => None,
    }
}

fn bbox2_union(x: Option<Box2<usize>>, y: Option<Box2<usize>>) -> Option<Box2<usize>> {
    [x, y]
        .iter()
        .cloned()
        .collect::<Box2UsizeUnion>()
        .into_box2()
}

fn scale_mat3(mut m: Matrix3<f32>, scale: f32) -> Matrix3<f32> {
    m.x.x *= scale;
    m.x.y *= scale;
    m.y.x *= scale;
    m.y.y *= scale;
    m.z.x *= scale;
    m.z.y *= scale;
    m
}

fn translate_neg_mat3(mut m: Matrix3<f32>, v: Vector2<f32>) -> Matrix3<f32> {
    m.z.x -= v.x;
    m.z.y -= v.y;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn empty_wnds() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let wnd1 = screen.new_wnd();
        let wnd2 = screen.new_wnd();

        screen.set_wnd_size(&wnd1, [20, 30]);
        screen.set_wnd_size(&wnd2, [40, 50]);

        dbg!(&wnd1);
        dbg!(&wnd2);

        debug_assert_eq!(
            screen.update_wnd(&wnd1),
            Some(box2! { min: [0, 0], max: [20, 30] })
        );
        debug_assert_eq!(
            screen.update_wnd(&wnd2),
            Some(box2! { min: [0, 0], max: [40, 50] })
        );

        for _ in 0..2 {
            debug_assert_eq!(screen.update_wnd(&wnd1), None);
            debug_assert_eq!(screen.update_wnd(&wnd2), None);
        }

        screen.remove_wnd(&wnd1);
        screen.remove_wnd(&wnd2);
    }

    // layer_ref_counting*
    // ----------------------------------------------------------------------
    // Validates the reference counting behaviour of layers.
    #[test]
    fn layer_ref_counting1() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer1 = screen.new_layer(Default::default());
        let layer2 = screen.new_layer(Default::default());

        assert_eq!(screen.layers.iter().count(), 2);

        screen.set_layer_attr(
            &layer1,
            iface::LayerAttrs {
                sublayers: Some(vec![layer2.clone()]),
                ..Default::default()
            },
        );

        screen.remove_layer(&layer2);
        assert_eq!(screen.layers.iter().count(), 2);

        screen.remove_layer(&layer1);
        assert_eq!(screen.layers.iter().count(), 0);
    }

    #[test]
    fn layer_ref_counting2() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer1 = screen.new_layer(Default::default());
        let layer2 = screen.new_layer(Default::default());

        assert_eq!(screen.layers.iter().count(), 2);

        let wnd = screen.new_wnd();
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        screen.remove_layer(&layer1);
        assert_eq!(screen.layers.iter().count(), 2);

        screen.set_wnd_layer(&wnd, Some(layer2.clone()));
        assert_eq!(screen.layers.iter().count(), 1);

        screen.remove_layer(&layer2);
        assert_eq!(screen.layers.iter().count(), 1);

        screen.remove_wnd(&wnd);
        assert_eq!(screen.layers.iter().count(), 0);
    }

    #[test]
    fn layer_ref_counting3() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer1 = screen.new_layer(Default::default());
        let layer2 = screen.new_layer(Default::default());
        let layer3 = screen.new_layer(Default::default());

        assert_eq!(screen.layers.iter().count(), 3);

        screen.set_layer_attr(
            &layer1,
            iface::LayerAttrs {
                sublayers: Some(vec![layer2.clone()]),
                ..Default::default()
            },
        );

        let wnd = screen.new_wnd();
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));
        // wnd -> layer1 -> layer2

        screen.update_wnd(&wnd);

        screen.set_layer_attr(
            &layer1,
            iface::LayerAttrs {
                sublayers: Some(vec![layer3.clone()]),
                ..Default::default()
            },
        );
        // wnd -> layer1 -> layer3

        screen.remove_layer(&layer2);
        assert_eq!(screen.layers.iter().count(), 3);

        screen.update_wnd(&wnd);
        assert_eq!(screen.layers.iter().count(), 2);
    }

    // root_update_*
    // ----------------------------------------------------------------------
    // A root layer is modified. After that, the calculated dirty region is
    // checked.
    #[test]
    fn root_update_content() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer1,
            iface::LayerAttrs {
                contents: Some(Some(TestBmp)),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [20, 30], max: [80, 50] })
        );
    }

    #[test]
    fn root_update_position() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            bg_color: Some([0.5, 0.6, 0.7, 0.8].into()),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer1,
            iface::LayerAttrs {
                bounds: Some(box2! { min: [40.0, 70.0], max: [90.0, 80.0] }),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [20, 30], max: [90, 80] })
        );
    }

    // sublayer_update_*
    // ----------------------------------------------------------------------
    // A sublayer of the root layer is modified. After that, the calculated
    // dirty region is checked.
    #[test]
    fn sublayer_update_content() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [30.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer2,
            iface::LayerAttrs {
                contents: Some(Some(TestBmp)),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [20, 30], max: [80, 50] })
        );
    }

    #[test]
    fn sublayer_update_position() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            bg_color: Some([0.5, 0.6, 0.7, 0.8].into()),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [30.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer2,
            iface::LayerAttrs {
                bounds: Some(box2! { min: [40.0, 70.0], max: [90.0, 80.0] }),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [20, 30], max: [90, 80] })
        );
    }

    // masked_sublayer_update_*
    // ----------------------------------------------------------------------
    // A sublayer of the root layer with `MASK_TO_BOUNDS` is modified. After
    // that, the calculated dirty region is checked.
    //
    //  (20, 30)
    //     ┌─────────────────────────┐
    //     │ (30, 40)                │layer2
    //     │    ┌────────┐           │
    //     │    │        │layer1     │
    //     │    │        │(root)     │
    //     │    └────────┘           │
    //     │         (60, 60)        │
    //     │                         │
    //     │                         │
    //     └─────────────────────────┘
    //                           (80, 50)
    #[test]
    fn masked_sublayer_update_content() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [30.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            flags: Some(iface::LayerFlags::MASK_TO_BOUNDS),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer2,
            iface::LayerAttrs {
                contents: Some(Some(TestBmp)),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [30, 40], max: [60, 50] })
        );
    }

    //  (20, 30)
    //     ┌──────────────────────────┐
    //     │ (30, 40)                 │layer2
    //     │   ┌────────────┐         │
    //     │   │(40, 50)    │         │
    //     │   │  ┌─────────┼─────────┼────┐
    //     │   │  │         │layer1   │    │layer2 (after)
    //     │   │  │         │(root)   │    │
    //     │   └──┼─────────┘         │    │
    //     │      │     (60, 60)      │    │
    //     │      │                   │    │
    //     └──────┼───────────────────┘    │
    //            │                (80, 50)│
    //            └────────────────────────┘
    //                             (90, 80)
    #[test]
    fn masked_sublayer_update_position() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            bg_color: Some([0.5, 0.6, 0.7, 0.8].into()),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [30.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            flags: Some(iface::LayerFlags::MASK_TO_BOUNDS),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer2,
            iface::LayerAttrs {
                bounds: Some(box2! { min: [40.0, 50.0], max: [90.0, 80.0] }),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [30, 40], max: [60, 60] })
        );
    }

    //         (20, 30)
    //            ┌──────────────────────────┐
    //   (10, 40) │                          │layer2
    //          ┌─┼────────────────┐         │
    //          │ │    (40, 50)    │         │
    //          │ │      ┌─────────┼─────────┼────┐
    //          │ │      │         │layer1   │    │layer2 (after)
    //          │ │      │         │(root)   │    │
    //          └─┼──────┼─────────┘         │    │
    //            │      │     (60, 60)      │    │
    //            │      │                   │    │
    //            └──────┼───────────────────┘    │
    //                   │                (80, 50)│
    //                   └────────────────────────┘
    //                                    (90, 80)
    #[test]
    fn masked_sublayer_update_position2() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            bg_color: Some([0.5, 0.6, 0.7, 0.8].into()),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [10.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            flags: Some(iface::LayerFlags::MASK_TO_BOUNDS),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer2,
            iface::LayerAttrs {
                bounds: Some(box2! { min: [40.0, 50.0], max: [90.0, 80.0] }),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [20, 40], max: [60, 60] })
        );
    }

    //         (20, 30)
    //            ┌──────────────────────────┐
    //   (10, 40) │                          │layer2
    //          ┌─┼────────────────┐         │
    //          │ │    (40, 50)    │         │
    //          │ │      ┌─────────┼─────────┼────┐
    //          │ │      │         │layer1   │    │layer2 (after)
    //          │ │      │         │(root)   │    │
    //          └─┼──────┼─────────┘         │    │
    //            │      │     (60, 60)      │    │
    //            │      │                   │    │
    //            └──────┼───────────────────┘    │
    //                   │                (80, 50)│
    //                   └────────────────────────┘
    //                                    (90, 80)
    // The difference from `masked_sublayer_update_position2` is the addition
    // of `set_wnd_dpi_scale`.
    #[test]
    fn masked_sublayer_update_position2_dpi_scale() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            bg_color: Some([0.5, 0.6, 0.7, 0.8].into()),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [10.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            flags: Some(iface::LayerFlags::MASK_TO_BOUNDS),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [200, 200]);
        screen.set_wnd_dpi_scale(&wnd, 2.0);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [200, 200] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer2,
            iface::LayerAttrs {
                bounds: Some(box2! { min: [40.0, 50.0], max: [90.0, 80.0] }),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [40, 80], max: [120, 120] })
        );
    }

    // root_opacity
    // ----------------------------------------------------------------------
    // The opacity of a root layer is modified. The root layer contains a
    // sublayer. The calculated dirty region is checked.
    #[test]
    fn root_opacity() {
        let mut screen: Screen<TestBmp> = Screen::new();

        let layer2 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [20.0, 30.0], max: [80.0, 50.0] }),
            contents: Some(Some(TestBmp)),
            ..Default::default()
        });
        let layer1 = screen.new_layer(iface::LayerAttrs {
            bounds: Some(box2! { min: [30.0, 40.0], max: [60.0, 60.0] }),
            sublayers: Some(vec![layer2.clone()]),
            ..Default::default()
        });

        let wnd = screen.new_wnd();
        screen.set_wnd_size(&wnd, [100, 100]);
        screen.set_wnd_layer(&wnd, Some(layer1.clone()));

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [0, 0], max: [100, 100] })
        );
        debug_assert_eq!(screen.update_wnd(&wnd), None);

        screen.set_layer_attr(
            &layer1,
            iface::LayerAttrs {
                opacity: Some(0.5),
                ..Default::default()
            },
        );

        dbg!(&screen);

        debug_assert_eq!(
            screen.update_wnd(&wnd),
            Some(box2! { min: [20, 30], max: [80, 50] })
        );
    }
}
