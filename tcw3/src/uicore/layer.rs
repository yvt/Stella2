use bitflags::bitflags;
use flags_macro::flags;

use super::{HView, ViewDirtyFlags, ViewFlags};
use crate::pal::{self, WM};

impl HView {
    pub(super) fn view_with_containing_layer(&self) -> Option<HView> {
        let mut view_or_not = (self.view.superview.borrow())
            .view()
            .and_then(|weak| weak.upgrade());
        while let Some(view) = view_or_not {
            if view.flags.contains(ViewFlags::LAYER_GROUP) {
                return Some(HView { view });
            }
            view_or_not = (view.superview.borrow())
                .view()
                .and_then(|weak| weak.upgrade());
        }
        None
    }

    fn enum_sublayers(&self, cb: &mut impl FnMut(&pal::HLayer)) {
        for layer in self.view.layers.borrow().iter() {
            cb(layer);
        }
        if !self.view.flags.contains(ViewFlags::LAYER_GROUP) {
            for subview in self.view.layout.borrow().subviews().iter() {
                subview.enum_sublayers(&mut *cb);
            }
        }
    }

    /// Call `ViewListener::update` on sublayers as necessary.
    ///
    /// Returns `true` if `layers` has changed. The return value is used to
    /// implement a recursive algorithm of `update_layers` itself.
    pub(super) fn update_layers(&self, wm: &WM) -> bool {
        let dirty = &self.view.dirty;

        let mut layers_changed = false;

        // Check subviews first
        let desc_flags = flags![ViewDirtyFlags::{
            DESCENDANT_UPDATE_EVENT | DESCENDANT_SUBLAYERS
        }];
        if dirty.get().intersects(desc_flags) {
            dirty.set(dirty.get() - desc_flags);

            for subview in self.view.layout.borrow().subviews().iter() {
                layers_changed |= subview.update_layers(wm);
            }
        }

        // If this is a layer group, then changes in the subtree of layers are
        // handled here
        if self.view.flags.contains(ViewFlags::LAYER_GROUP) {
            if layers_changed {
                self.set_dirty_flags(ViewDirtyFlags::SUBLAYERS);
                layers_changed = false;
            }
        }

        let update_flags = flags![ViewDirtyFlags::{UPDATE_EVENT | SUBLAYERS}];
        if dirty.get().intersects(update_flags) {
            let mut layers = self.view.layers.borrow_mut();

            let mut ctx = UpdateCtx {
                reason: UpdateReason::empty(),
                sublayers: None,
                layers: &mut *layers,
                layers_updated: false,
            };

            if dirty.get().intersects(ViewDirtyFlags::UPDATE_EVENT) {
                ctx.reason |= UpdateReason::PEND_UPDATE;
            }
            if dirty.get().intersects(ViewDirtyFlags::SUBLAYERS) {
                ctx.reason |= UpdateReason::SUBLAYERS_CHANGE;

                debug_assert!(self.view.flags.contains(ViewFlags::LAYER_GROUP));

                // Compile a list of direct sublayers
                let mut sublayers = Vec::new();

                for subview in self.view.layout.borrow().subviews().iter() {
                    subview.enum_sublayers(&mut |layer| sublayers.push(layer.clone()));
                }

                ctx.sublayers = Some(sublayers);
            }

            dirty.set(dirty.get() - update_flags);

            self.view.listener.borrow().update(wm, self, &mut ctx);

            if ctx.layers_updated {
                layers_changed = true;
            }
        }

        layers_changed
    }
}

/// The context for [`ViewListener::update`].
///
/// [`ViewListener::update`]: crate::uicore::ViewListener::update
pub struct UpdateCtx<'a> {
    reason: UpdateReason,
    sublayers: Option<Vec<pal::HLayer>>,
    layers: &'a mut Vec<pal::HLayer>,
    layers_updated: bool,
}

impl<'a> UpdateCtx<'a> {
    /// Get flags indicating why `update` was called.
    pub fn reason(&self) -> UpdateReason {
        self.reason
    }

    /// Get a set of sublayers associated with subviews.
    ///
    /// This method is valid only for layers with [`ViewFlags::LAYER_GROUP`].
    ///
    /// When the referred value is `Some()`, the view's sublayers must be
    /// updated with that value. The client may `take` this `Option`.
    pub fn sublayers(&mut self) -> &mut Option<Vec<pal::HLayer>> {
        &mut self.sublayers
    }

    /// Set a new set of layers associated with a view.
    ///
    /// You shouldn't call this if the set is identical to a previously
    /// known one. One way to check this is to check the number of elements in
    /// `layers()`, which is initially zero.
    pub fn set_layers(&mut self, layers: Vec<pal::HLayer>) {
        *self.layers = layers;
        self.layers_updated = true;
    }

    /// Get a set of layers associated with a view.
    pub fn layers(&self) -> &[pal::HLayer] {
        &self.layers[..]
    }
}

bitflags! {
    /// Describes the reason why [`ViewListener::update`] was called.
    ///
    /// [`ViewListener::update`]: crate::uicore::ViewListener::update
    pub struct UpdateReason: u32 {
        /// [`View::pend_update`] was called.
        const PEND_UPDATE = 1 << 0;

        /// The set of sublayers has changed. [`UpdateCtx::sublayers`] returns
        /// a mutable reference to a `Some` value.
        ///
        /// This bit is valid only for layers with [`ViewFlags::LAYER_GROUP`].
        const SUBLAYERS_CHANGE = 1 << 1;
    }
}
