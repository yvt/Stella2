use cggeom::prelude::*;
use cgmath::{prelude::*, Matrix4};
use cocoa::{
    base::{id, nil},
    quartzcore::CALayer,
};
use core_graphics::geometry::CGPoint;
use iterpool::{Pool, PoolPtr};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::{Cell, RefCell};

use super::super::iface::LayerFlags;
use super::{
    drawutils::{
        ca_transform_3d_from_matrix4, cg_color_from_rgbaf32, cg_rect_from_box2,
        extend_matrix3_with_identity_z,
    },
    LayerAttrs, MtSticky, Wm,
};

static LAYER_POOL: MtSticky<RefCell<Pool<Layer>>> = MtSticky::new(RefCell::new(Pool::new()));

static DELETION_QUEUE: MtSticky<RefCell<Vec<HLayer>>> = MtSticky::new(RefCell::new(Vec::new()));

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HLayer {
    /// The pointer to a `Layer` in `LAYER_POOL`.
    ptr: PoolPtr,
}

struct Layer {
    ca_layer: CALayer,

    /// Deferred attribute updates to be flushed.
    ///
    /// It only has `Some` values for updated attributes.
    /// The new state after the next transaction are calculated like this:
    /// `attrs_current.override_with(attrs_diff)`.
    attrs_diff: RefCell<LayerAttrs>,

    /// The current sublayers of `CALayer` expressed in `HLayer`s. In other
    /// words, it tracks the committed state of the `sublayers` attribute.
    sublayers: RefCell<Vec<HLayer>>,

    /// This layer of one of its sublayers have pending updates in `attrs_diff`.
    /// It may have a false-positive.
    needs_update: Cell<bool>,

    /// The superlayer.
    ///
    /// It's immediately updated when `set_attrs` is called.
    /// Thus, it's based on the reverse mapping of
    /// `attrs_diff.sublayers.unwrap_or(sublayers)`.
    superlayer: Cell<Option<HLayer>>,

    /// `true` if `remove` was called, but the layer can't be deleted because
    /// `superlayer` is not `None`. It'll be deleted when its detached from the
    /// superlayer.
    pending_deletion: Cell<bool>,
}

impl Layer {
    pub fn new(_: Wm) -> Self {
        // `CALayer::new` wraps `[CALayer layer]`, which `autorelease`s itself,
        // so we have to `retain` the returned `CALayer` to prevent it
        // from being dealloced prematurely. (This clearly should be done by
        // `CALayer::new()`, though...)
        let ca_layer = CALayer::new();
        let () = unsafe { msg_send![ca_layer.id(), retain] };

        Self {
            ca_layer,
            attrs_diff: RefCell::new(LayerAttrs::default()),
            sublayers: RefCell::new(Vec::new()),
            needs_update: Cell::new(false),
            superlayer: Cell::new(None),
            pending_deletion: Cell::new(false),
        }
    }
}

impl HLayer {
    pub(super) fn new(wm: Wm, attrs: LayerAttrs) -> Self {
        let layer = Layer::new(wm);
        let ptr = LAYER_POOL.get_with_wm(wm).borrow_mut().allocate(layer);
        let this = Self { ptr };
        this.set_attrs(wm, attrs);
        this
    }

    pub(super) fn remove(&self, wm: Wm) {
        let mut layer_pool = LAYER_POOL.get_with_wm(wm).borrow_mut();

        let this_layer: &Layer = &layer_pool[self.ptr];

        this_layer.pending_deletion.set(true);

        if this_layer.superlayer.get().is_none() {
            // We can delete the layer immediately if it doesn't
            let mut deletion_queue = DELETION_QUEUE.get_with_wm(wm).borrow_mut();
            debug_assert!(deletion_queue.is_empty());

            self.handle_pending_deletion(&layer_pool, wm, &mut deletion_queue);

            for hlayer in deletion_queue.drain(..) {
                layer_pool.deallocate(hlayer.ptr);
            }
        }
    }

    /// Delete the layer if `pending_deletion == true`. This might cause cascade
    /// deletion for sublayers with `pending_deletion.get() == true`
    ///
    /// The method doesn't actually do the deletion - it just adds the layers to
    /// be deleted to `deletion_queue`.
    fn handle_pending_deletion(
        &self,
        layer_pool: &Pool<Layer>,
        wm: Wm,
        deletion_queue: &mut Vec<HLayer>,
    ) {
        let this_layer: &Layer = &layer_pool[self.ptr];

        if !this_layer.pending_deletion.get() {
            return;
        }

        deletion_queue.push(*self);

        let attrs_diff = this_layer.attrs_diff.borrow();
        let committed_sublayers = this_layer.sublayers.borrow();
        let sublayers = attrs_diff
            .sublayers
            .as_ref()
            .unwrap_or(&*committed_sublayers);

        for hlayer in sublayers.iter() {
            hlayer.handle_pending_deletion(layer_pool, wm, &mut *deletion_queue);
        }
    }

    pub(super) fn set_attrs(&self, wm: Wm, attrs: LayerAttrs) {
        let mut layer_pool = LAYER_POOL.get_with_wm(wm).borrow_mut();
        let layer_pool = &mut *layer_pool; // enable split borrow

        let update_sublayers = attrs.sublayers.is_some();

        if update_sublayers {
            // Disconnect sublayers first
            let this_layer: &Layer = &layer_pool[self.ptr];
            let attrs_diff = this_layer.attrs_diff.borrow();
            let committed_sublayers = this_layer.sublayers.borrow();
            let sublayers = attrs_diff
                .sublayers
                .as_ref()
                .unwrap_or(&*committed_sublayers);

            let mut deletion_queue = DELETION_QUEUE.get_with_wm(wm).borrow_mut();
            debug_assert!(deletion_queue.is_empty());

            for hlayer in sublayers.iter() {
                debug_assert_eq!(layer_pool[hlayer.ptr].superlayer.get(), Some(*self));
                layer_pool[hlayer.ptr].superlayer.set(None);
                hlayer.handle_pending_deletion(&layer_pool, wm, &mut deletion_queue);
            }
        }

        layer_pool[self.ptr]
            .attrs_diff
            .borrow_mut()
            .override_with(attrs);

        if update_sublayers {
            // Connect sublayers
            let this_layer: &Layer = &layer_pool[self.ptr];
            let attrs_diff = this_layer.attrs_diff.borrow();
            let sublayers = attrs_diff.sublayers.as_ref().unwrap();
            for hlayer in sublayers.iter() {
                debug_assert_eq!(
                    layer_pool[hlayer.ptr].superlayer.get(),
                    None,
                    "layers only can have up to one parents."
                );
                layer_pool[hlayer.ptr].superlayer.set(Some(*self));
            }
        }

        // Propagate `needs_update`
        {
            let mut layer: &Layer = &layer_pool[self.ptr];
            while !layer.needs_update.get() {
                layer.needs_update.set(true);
                if let Some(sup) = layer.superlayer.get() {
                    layer = &layer_pool[sup.ptr];
                } else {
                    break;
                }
            }
        }

        // Flush deferred layer deletion
        if update_sublayers {
            let mut deletion_queue = DELETION_QUEUE.get_with_wm(wm).borrow_mut();

            for hlayer in deletion_queue.drain(..) {
                layer_pool.deallocate(hlayer.ptr);
            }
        }
    }

    /// Apply deferred property updates to the underlying `CALayer`.
    ///
    /// The operation is performed recursively on sublayers.
    fn flush_with_layer_pool(&self, layer_pool: &Pool<Layer>) {
        let this_layer: &Layer = &layer_pool[self.ptr];

        // Update this layer's local properties
        if !this_layer.needs_update.get() {
            return;
        }
        this_layer.needs_update.set(false);

        let mut attrs_diff = this_layer.attrs_diff.borrow_mut();
        let mut sublayers = this_layer.sublayers.borrow_mut();

        if let Some(value) = attrs_diff.transform.take() {
            let m: Matrix4<f64> = extend_matrix3_with_identity_z(value).cast().unwrap();
            this_layer
                .ca_layer
                .set_transform(&ca_transform_3d_from_matrix4(m));

            // Our `transform` doesn't affect sublayers
            let m_inv: Matrix4<f64> = extend_matrix3_with_identity_z(value.invert().unwrap())
                .cast()
                .unwrap();
            this_layer
                .ca_layer
                .set_sublayer_transform(ca_transform_3d_from_matrix4(m_inv));
        }

        if let Some(value) = attrs_diff.contents.take() {
            // Be careful - Do not drop `value` until `set_contents` because
            // the following `cg_image` is just a `id`, not a smart pointer
            use std::mem::transmute;
            let cg_image = if let Some(ref bitmap) = value {
                // `CGImageRef` → `id`
                unsafe { transmute(&*bitmap.cg_image) }
            } else {
                nil
            };
            unsafe { this_layer.ca_layer.set_contents(cg_image) };
        }

        if let Some(value) = attrs_diff.bounds.take() {
            this_layer
                .ca_layer
                .set_bounds(&cg_rect_from_box2(value.cast().unwrap()));

            // Place the anchor at the point whose local coordinates are (0, 0)
            let size = value.size();
            this_layer.ca_layer.set_anchor_point(&CGPoint::new(
                (-value.min.x / size.x) as f64,
                (-value.min.y / size.y) as f64,
            ));
        }

        if let Some(value) = attrs_diff.contents_center.take() {
            this_layer
                .ca_layer
                .set_contents_center(&cg_rect_from_box2(value.cast().unwrap()));
        }

        if let Some(value) = attrs_diff.contents_scale.take() {
            this_layer.ca_layer.set_contents_scale(value as f64);
        }

        if let Some(value) = attrs_diff.bg_color.take() {
            let cf_color = if value.a > 0.0 {
                let c = cg_color_from_rgbaf32(value);
                std::mem::forget(c.clone());
                Some(c)
            } else {
                None
            };
            this_layer.ca_layer.set_background_color(cf_color);
        }

        if let Some(value) = attrs_diff.sublayers.take() {
            *sublayers = value;

            let ca_sub_layers: Vec<_> = sublayers
                .iter()
                .map(|hlayer| layer_pool[hlayer.ptr].ca_layer.id())
                .collect();

            // Autoreleased `NSArray`
            let array: id = unsafe {
                msg_send![
                    class!(NSArray),
                    arrayWithObjects:ca_sub_layers.as_ptr()
                               count:ca_sub_layers.len()
                ]
            };

            let () = unsafe { msg_send![this_layer.ca_layer.id(), setSublayers: array] };
        }

        if let Some(value) = attrs_diff.opacity.take() {
            this_layer.ca_layer.set_opacity(value);
        }

        if let Some(value) = attrs_diff.flags.take() {
            this_layer
                .ca_layer
                .set_masks_to_bounds(value.contains(LayerFlags::MASK_TO_BOUNDS));
        }

        // Recurse into sublayers
        for hlayer in sublayers.iter() {
            hlayer.flush_with_layer_pool(layer_pool);
        }
    }

    /// Apply deferred property updates to the underlying `CALayer`.
    ///
    /// The operation is performed recursively on sublayers.
    /// `wm` is used for compile-time thread checking.
    pub(super) fn flush(&self, wm: Wm) {
        self.flush_with_layer_pool(&LAYER_POOL.get_with_wm(wm).borrow());
    }

    /// Get the `CALayer` of a layer.
    ///
    /// `wm` is used for compile-time thread checking.
    pub(super) fn ca_layer(&self, wm: Wm) -> id {
        LAYER_POOL.get_with_wm(wm).borrow()[self.ptr].ca_layer.id()
    }
}
