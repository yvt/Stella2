use cggeom::prelude::*;
use cgmath::{prelude::*, Matrix4};
use cocoa::{
    base::{id, nil},
    quartzcore::{transaction, CALayer},
};
use core_graphics::geometry::CGPoint;
use leakypool::{LeakyPool, PoolPtr};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::RefCell;

use super::super::iface::LayerFlags;
use super::{
    drawutils::{
        ca_transform_3d_from_matrix4, cg_color_from_rgbaf32, cg_rect_from_box2,
        extend_matrix3_with_identity_z,
    },
    LayerAttrs, MtSticky, Wm,
};

static LAYER_POOL: MtSticky<RefCell<LeakyPool<Layer>>> =
    MtSticky::new(RefCell::new(LeakyPool::new()));

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HLayer {
    /// The pointer to a `Layer` in `LAYER_POOL`.
    ptr: PoolPtr<Layer>,
}

struct Layer {
    ca_layer: CALayer,
}

impl Layer {
    pub fn new(_: Wm) -> Self {
        // `CALayer::new` wraps `[CALayer layer]`, which `autorelease`s itself,
        // so we have to `retain` the returned `CALayer` to prevent it
        // from being dealloced prematurely. (This clearly should be done by
        // `CALayer::new()`, though...)
        let ca_layer = CALayer::new();
        let () = unsafe { msg_send![ca_layer.id(), retain] };

        Self { ca_layer }
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
        layer_pool.deallocate(self.ptr).unwrap();
    }

    pub(super) fn set_attrs(&self, wm: Wm, attrs: LayerAttrs) {
        let mut layer_pool = LAYER_POOL.get_with_wm(wm).borrow_mut();
        let layer_pool = &mut *layer_pool; // enable split borrow

        let this_layer: &Layer = &layer_pool[self.ptr];

        transaction::set_animation_duration(0.0);

        if let Some(value) = attrs.transform {
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

        if let Some(value) = attrs.contents {
            // Be careful - Do not drop `value` until `set_contents` because
            // the following `cg_image` is just a `id`, not a smart pointer
            let cg_image = if let Some(ref bitmap) = value {
                // `CGImageRef` → `id`
                &*bitmap.cg_image as *const _ as id
            } else {
                nil
            };
            unsafe { this_layer.ca_layer.set_contents(cg_image) };
        }

        if let Some(value) = attrs.bounds {
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

        if let Some(value) = attrs.contents_center {
            this_layer
                .ca_layer
                .set_contents_center(&cg_rect_from_box2(value.cast().unwrap()));
        }

        if let Some(value) = attrs.contents_scale {
            this_layer.ca_layer.set_contents_scale(value as f64);
        }

        if let Some(value) = attrs.bg_color {
            let cf_color = if value.a > 0.0 {
                let c = cg_color_from_rgbaf32(value);
                std::mem::forget(c.clone());
                Some(c)
            } else {
                None
            };
            this_layer.ca_layer.set_background_color(cf_color);
        }

        if let Some(value) = attrs.sublayers {
            let ca_sub_layers: Vec<_> = value
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

        if let Some(value) = attrs.opacity {
            this_layer.ca_layer.set_opacity(value);
        }

        if let Some(value) = attrs.flags {
            this_layer
                .ca_layer
                .set_masks_to_bounds(value.contains(LayerFlags::MASK_TO_BOUNDS));
        }
    }

    /// Get the `CALayer` of a layer.
    ///
    /// `wm` is used for compile-time thread checking.
    pub(super) fn ca_layer(&self, wm: Wm) -> id {
        LAYER_POOL.get_with_wm(wm).borrow()[self.ptr].ca_layer.id()
    }
}
