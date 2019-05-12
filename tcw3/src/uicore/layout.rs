use as_any::AsAny;
use cggeom::{prelude::*, Box2};
use cgmath::{vec2, Vector2};
use std::{fmt, rc::Rc};

use super::{HView, ViewDirtyFlags};
use crate::pal::WM;

/// Represents a type defining the positioning of subviews.
///
/// Associated with a single view (referred to by [`HView`]) via [`set_layout`],
/// a layout controls the layout properties of the view as well as the
/// arrangement of its subviews.
///
/// [`HView`]: crate::uicore::HView
/// [`set_layout`]: crate::uicore::HView::set_layout
///
/// `Layout` is logically immutable. That means the return values of these
/// methods only can change based on input values. You should always
/// re-create `Layout` objects if you want to modify its parameters.
pub trait Layout: AsAny {
    /// Get the subviews of a layout.
    ///
    /// The returned value must be constant.
    fn subviews(&self) -> &[HView];

    /// Calculate the [`SizeTraits`] for a layout.
    ///
    /// The returned value must be a function of `self` and `SizeTraits`s of
    /// subviews retrieved via `ctx`.
    fn size_traits(&self, ctx: &LayoutCtx) -> SizeTraits;

    /// Position the subviews of a layout.
    ///
    /// `size` is the size of the view associated with the layout.
    ///
    /// The callee must position every subview using [`LayoutCtx::set_subview_frame`].
    /// The result must be a function of `self`, `size`, and `SizeTraits`es of
    /// subviews retrieved via [`LayoutCtx::subview_size_traits`].
    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>);

    /// Return `true` if `self.subviews()` is identical to `other.subviews()`
    /// with a potential negative positive. *Reordering counts as difference.*
    ///
    /// This method is used to expedite the process of swapping layouts if they
    /// share an identical set of subviews.
    ///
    /// It can be assumed that the pointer values of `self` and `other` are
    /// never equal to each other.
    fn has_same_subviews(&self, _other: &dyn Layout) -> bool {
        false
    }
}

impl fmt::Debug for dyn Layout {
    /// Output the address of `self` and `self.subviews()`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Layout")
            .field("ptr", &(self as *const _))
            .field("subviews()", &self.subviews())
            .finish()
    }
}

/// `Layout` with no subviews, no size limitation, and 0x0 as the preferred size.
#[derive(Debug, Clone, Copy)]
pub struct DefaultLayout;

impl Layout for DefaultLayout {
    fn subviews(&self) -> &[HView] {
        &[]
    }
    fn size_traits(&self, _: &LayoutCtx) -> SizeTraits {
        SizeTraits::default()
    }
    fn arrange(&self, _: &mut LayoutCtx<'_>, _: Vector2<f32>) {}
    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        // See if `other` has the same type
        as_any::Downcast::is::<Self>(other)
    }
}

/// Minimum, maximum, and preferred sizes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeTraits {
    pub min: Vector2<f32>,
    pub max: Vector2<f32>,
    pub preferred: Vector2<f32>,
}

impl Default for SizeTraits {
    /// Return `Self { min: (0, 0), max: (∞, ∞), preferred: (0, 0) }`.
    fn default() -> Self {
        use std::f32::INFINITY;
        Self {
            min: Vector2::new(0.0, 0.0),
            max: Vector2::new(INFINITY, INFINITY),
            preferred: Vector2::new(0.0, 0.0),
        }
    }
}

impl HView {
    /// Get the frame (bounding rectangle) of a view in the superview's
    /// coordinate space.
    ///
    /// This method might return an out-dated value unless it's called under
    /// certain circumstances. The layout system arranges to make sure that all
    /// views in a window have up-to-date `frame` coordinates before calling
    /// [`ViewListener::position`], a handler method for detecting changes in
    /// `frame`. Thus, `ViewListener::position` is the only place where the
    /// final value of `frame` can be retrieved reliably.
    ///
    /// [`ViewListener::position`]: crate::uicore::ViewListener::position
    pub fn frame(&self) -> Box2<f32> {
        self.view.frame.get()
    }

    /// Get the frame (bounding rectangle) of a view in the containing window's
    /// coordinate space.
    ///
    /// This method might return an out-dated value unless it's called under
    /// certain circumstances. See [`frame`] for details.
    ///
    /// [`frame`]: crate::uicore::HView::frame
    pub fn global_frame(&self) -> Box2<f32> {
        self.view.global_frame.get()
    }

    /// Update `size_traits` of a view. This implements the *up phase* of the
    /// layouting algorithm.
    ///
    /// Returns `true` if `size_traits` has changed. The return value is used to
    /// implement a recursive algorithm of `update_size_traits` itself.
    pub(super) fn update_size_traits(&self) -> bool {
        let dirty = &self.view.dirty;
        let layout = self.view.layout.borrow();

        if dirty
            .get()
            .intersects(ViewDirtyFlags::DESCENDANT_SIZE_TRAITS)
        {
            dirty.set(dirty.get() - ViewDirtyFlags::DESCENDANT_SIZE_TRAITS);

            // Check `size_traits` of subviews first
            let mut needs_recalculate = false;
            for subview in layout.subviews().iter() {
                if subview.update_size_traits() {
                    needs_recalculate = true;
                }
            }

            // If they change, ours might change, too
            if needs_recalculate {
                dirty.set(dirty.get() | ViewDirtyFlags::SIZE_TRAITS);
            }
        }

        if dirty.get().intersects(ViewDirtyFlags::SIZE_TRAITS) {
            dirty.set(dirty.get() - ViewDirtyFlags::SIZE_TRAITS);

            let new_size_traits = layout.size_traits(&LayoutCtx { active_view: self });

            // See if `size_traits` has changed
            if new_size_traits != self.view.size_traits.get() {
                self.view.size_traits.set(new_size_traits);
                return true;
            }
        }

        false
    }

    /// Update `frame` of subviews, assuming `self` has an up-to-date value of
    /// `frame` and `global_frame`. This implements the *down phase* of the
    /// layouting algorithm.
    ///
    /// During the process, it sets `POSITION_EVENT` dirty bit as necessary.
    pub(super) fn update_subview_frames(&self) {
        let dirty = &self.view.dirty;
        let layout = self.view.layout.borrow();

        if dirty.get().intersects(ViewDirtyFlags::SUBVIEWS_FRAME) {
            dirty.set(dirty.get() - ViewDirtyFlags::SUBVIEWS_FRAME);

            // Invoke the `Layout` to reposition the subviews.
            // It'll call `set_subview_frame` and set `DESCENDANT_SUBVIEWS_FRAME`
            // on `self` and `SUBVIEWS_FRAME` on the subviews.
            layout.arrange(
                &mut LayoutCtx { active_view: self },
                self.view.frame.get().size(),
            );
        }

        if dirty
            .get()
            .intersects(ViewDirtyFlags::DESCENDANT_SUBVIEWS_FRAME)
        {
            dirty.set(dirty.get() - ViewDirtyFlags::DESCENDANT_SUBVIEWS_FRAME);

            for subview in layout.subviews().iter() {
                subview.update_subview_frames();
            }
        }
    }

    /// Call `ViewListener::position` for subviews as necessary.
    pub(super) fn flush_position_event(&self, wm: WM) {
        fn traverse(this: &HView, cb: &mut impl FnMut(&HView)) {
            let dirty = &this.view.dirty;
            let layout = this.view.layout.borrow();

            if dirty.get().intersects(ViewDirtyFlags::POSITION_EVENT) {
                dirty.set(dirty.get() - ViewDirtyFlags::POSITION_EVENT);
                cb(this);
            }

            if dirty
                .get()
                .intersects(ViewDirtyFlags::DESCENDANT_POSITION_EVENT)
            {
                dirty.set(dirty.get() - ViewDirtyFlags::DESCENDANT_POSITION_EVENT);

                for subview in layout.subviews().iter() {
                    traverse(subview, &mut *cb);
                }
            }
        }

        // Construct the list of `HView`s first so the callback functions
        // do not panic with an "already borrowed" error attempting to modify
        // layouts
        let mut list = Vec::new();
        traverse(self, &mut |hview| list.push(hview.clone()));

        for view in list.iter() {
            view.view.listener.borrow().position(wm, view);
        }
    }
}

/// The context for [`Layout::arrange`] and [`Layout::size_traits`].
pub struct LayoutCtx<'a> {
    active_view: &'a HView,
}

impl<'a> LayoutCtx<'a> {
    /// Get `SizeTraits` for a subview `hview`.
    pub fn subview_size_traits(&self, hview: &HView) -> SizeTraits {
        self.ensure_subview(hview);
        hview.view.size_traits.get()
    }

    /// Set the frame (bounding rectangle) of a subview `hview`.
    ///
    /// This method only can be called from [`Layout::arrange`].
    pub fn set_subview_frame(&mut self, hview: &HView, frame: Box2<f32>) {
        self.ensure_subview(hview);

        // Local position
        if frame.size() != hview.view.frame.get().size() {
            hview.set_dirty_flags(ViewDirtyFlags::SUBVIEWS_FRAME);
            self.active_view
                .set_dirty_flags(ViewDirtyFlags::DESCENDANT_SUBVIEWS_FRAME);
        }

        hview.view.frame.set(frame);

        // Global position
        let global_offset = self.active_view.view.global_frame.get().min;
        let global_frame = frame.translate(vec2(global_offset.x, global_offset.y));

        if global_frame != hview.view.global_frame.get() {
            hview.view.global_frame.set(global_frame);
            hview.set_dirty_flags(ViewDirtyFlags::POSITION_EVENT);
            self.active_view
                .set_dirty_flags(ViewDirtyFlags::DESCENDANT_POSITION_EVENT);
        }
    }

    /// Panic if `hview` is not a subview of the active view and
    /// debug assertions are enabled.
    fn ensure_subview(&self, hview: &HView) {
        debug_assert_eq!(
            *hview.view.superview.borrow(),
            Rc::downgrade(&self.active_view.view),
            "the view is not a subview"
        );
    }
}
