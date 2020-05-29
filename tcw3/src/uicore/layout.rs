use as_any::AsAny;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{vec2, Point2, Vector2};
use flags_macro::flags;
use log::trace;
use rc_borrow::RcBorrow;
use std::{fmt, rc::Rc};

use super::{HView, HViewRef, ViewDirtyFlags, ViewFlags};
use crate::pal::Wm;

/// Represents a type defining the positioning of subviews.
///
/// Associated with a single view (referred to by [`HView`]) via [`set_layout`],
/// a layout controls the layout properties of the view as well as the
/// arrangement of its subviews.
///
/// [`HView`]: crate::uicore::HView
/// [`set_layout`]: crate::uicore::HViewRef::set_layout
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
    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits;

    /// Position the subviews of a layout.
    ///
    /// `size` is the size of the view associated with the layout. This value
    /// is bounded by the `SizeTraits` returned by `self.size_traits(ctx)`.
    /// However, the implementation must be prepared to gracefully handle
    /// an out-of-range value of `size` caused by rounding errors and/or
    /// unsatisfiable constraints.
    ///
    /// The callee must position every subview using [`LayoutCtx::set_subview_frame`].
    /// The result must be a function of `self`, `size`, and `SizeTraits`es of
    /// subviews retrieved via [`LayoutCtx::subview_size_traits`].
    ///
    /// The layout engine needs to know the view's `SizeTraits` before
    /// determining its size, thus whenever a subview's `SizeTraits` is updated,
    /// `size_traits` is called before `arrange` is called for the next time.
    /// This behaviour can be utilized by updating a `Layout`'s internal cache
    /// when `size_traits` is called.
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

impl<T: Layout + 'static> From<T> for Box<dyn Layout> {
    fn from(x: T) -> Box<dyn Layout> {
        Box::new(x)
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
impl Layout for () {
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

impl SizeTraits {
    /// Update `min` with a new value and return a new `SizeTraits`.
    pub fn with_min(self, min: Vector2<f32>) -> Self {
        Self { min, ..self }
    }

    /// Update `max` with a new value and return a new `SizeTraits`.
    pub fn with_max(self, max: Vector2<f32>) -> Self {
        Self { max, ..self }
    }

    /// Update `preferred` with a new value and return a new `SizeTraits`.
    pub fn with_preferred(self, preferred: Vector2<f32>) -> Self {
        Self { preferred, ..self }
    }
}

impl HViewRef<'_> {
    /// Get the frame (bounding rectangle) of a view in the superview's
    /// coordinate space.
    ///
    /// This method might return an out-dated value unless it's called under
    /// certain circumstances. The layout system arranges to make sure that all
    /// views in a window have up-to-date `frame` coordinates before calling
    /// [`ViewListener::position`], a handler method for detecting changes in
    /// `frame`. Thus, `ViewListener::position` and [`ViewListener::update`]
    /// (which is pended as necessary by `position`) are the only place where
    /// the final value of `frame` can be retrieved reliably.
    ///
    /// [`ViewListener::position`]: crate::uicore::ViewListener::position
    /// [`ViewListener::update`]: crate::uicore::ViewListener::update
    pub fn frame(self) -> Box2<f32> {
        self.view.frame.get()
    }

    /// Get the frame (bounding rectangle) of a view in the containing window's
    /// coordinate space.
    ///
    /// This method might return an out-dated value unless it's called under
    /// certain circumstances. See [`frame`] for details.
    ///
    /// [`frame`]: crate::uicore::HView::frame
    pub fn global_frame(self) -> Box2<f32> {
        self.view.global_frame.get()
    }

    /// Get the visible portion of `global_frame` in the containing window's
    /// coordinate space.
    ///
    /// `global_visible_frame` represents the intersection of `global_frame` of
    /// all clipping ancestors (viz., those having [`CLIP_VISIBLE_FRAME`]). The
    /// resulting rectangle may be empty.
    ///
    /// This method is useful for restricting the painted region of a view to
    /// the inside of a visible portion.
    ///
    /// This method might return an out-dated value unless it's called under
    /// certain circumstances. See [`frame`] for details.
    ///
    /// [`CLIP_VISIBLE_FRAME`]: crate::uicore::ViewFlags::CLIP_VISIBLE_FRAME
    /// [`frame`]: crate::uicore::HView::frame
    pub fn global_visible_frame(self) -> Box2<f32> {
        self.view.global_visible_frame.get()
    }

    /// Update `size_traits` of a view. This implements the *up phase* of the
    /// layouting algorithm.
    ///
    /// Returns `true` if `size_traits` has changed. The return value is used to
    /// implement a recursive algorithm of `update_size_traits` itself.
    pub(super) fn update_size_traits(self) -> bool {
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
                if subview.as_ref().update_size_traits() {
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

            let new_size_traits = layout.size_traits(&LayoutCtx {
                active_view: self,
                new_layout: None,
                #[cfg(debug_assertions)]
                is_arranging: false,
            });

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
    ///
    /// It's possible for a layout to assign a new layout by calling
    /// `LayoutCtx::set_layout`. When this happens, relevant dirty flags are
    /// set on ancestor views as if `HView::set_layout` is called as usual. The
    /// caller must detect this kind of situation and take an appropriate action.
    pub(super) fn update_subview_frames(self) {
        let dirty = &self.view.dirty;
        let layout = self.view.layout.borrow();

        let may_pend_position = dirty
            .get()
            .intersects(flags![ViewDirtyFlags::{SUBVIEWS_FRAME | DESCENDANT_SUBVIEWS_FRAME}]);

        if dirty.get().intersects(ViewDirtyFlags::SUBVIEWS_FRAME) {
            dirty.set(dirty.get() - ViewDirtyFlags::SUBVIEWS_FRAME);

            #[cfg(debug_assertions)]
            for hview in layout.subviews().iter() {
                hview.view.has_frame.set(false);
            }

            // Invoke the `Layout` to reposition the subviews.
            // It'll call `set_subview_frame` and set `DESCENDANT_SUBVIEWS_FRAME`
            // on `self` and `SUBVIEWS_FRAME` on the subviews.
            let mut ctx = LayoutCtx {
                active_view: self,
                new_layout: None,
                #[cfg(debug_assertions)]
                is_arranging: true,
            };
            layout.arrange(&mut ctx, self.view.frame.get().size());

            if let Some(new_layout) = ctx.new_layout.take() {
                // The layout asked replacement of layouts.
                drop(layout);
                self.set_layout(new_layout);
                return;
            }

            #[cfg(debug_assertions)]
            for hview in layout.subviews().iter() {
                assert!(
                    hview.view.has_frame.get(),
                    "`arrange` did not call `set_subview_frame` for the view {:?} ",
                    hview,
                );
            }
        }

        if dirty
            .get()
            .intersects(ViewDirtyFlags::DESCENDANT_SUBVIEWS_FRAME)
        {
            dirty.set(dirty.get() - ViewDirtyFlags::DESCENDANT_SUBVIEWS_FRAME);

            for subview in layout.subviews().iter() {
                subview.as_ref().update_subview_frames();
            }
        }

        if may_pend_position {
            let mut new_position_dirty = ViewDirtyFlags::empty();

            for subview in layout.subviews().iter() {
                new_position_dirty |=
                    subview.view.dirty.get() & ViewDirtyFlags::DESCENDANT_POSITION_EVENT;
            }

            // Propagate `DESCENDANT_POSITION_EVENT`
            dirty.set(dirty.get() | new_position_dirty);
        }
    }

    /// Call `ViewListener::position` for subviews as necessary.
    pub(super) fn flush_position_event(self, wm: Wm) {
        #[derive(Copy, Clone)]
        #[repr(align(16))]
        struct Ctx {
            clip: Box2<f32>,
            global_offset: Point2<f32>,
            extra_flags: ViewDirtyFlags,
        }

        fn update_global_frame(this: HViewRef<'_>, ctx: &Ctx) {
            // Global position
            let frame = this.view.frame.get();
            let global_offset = ctx.global_offset;
            let global_frame = frame.translate(vec2(global_offset.x, global_offset.y));
            this.view.global_frame.set(global_frame);

            // Clipped global position
            this.view.global_visible_frame.set(box2! {
                min: ctx.clip.min.element_wise_max(&global_frame.min),
                max: ctx.clip.max.element_wise_min(&global_frame.max),
            });
        }

        fn transform_ctx_for_subviews(this: HViewRef<'_>, ctx: &mut Ctx) {
            let global_frame = this.view.global_frame.get();
            ctx.global_offset = global_frame.min;
            if (this.view.flags.get()).contains(ViewFlags::CLIP_VISIBLE_FRAME) {
                ctx.clip.min = ctx.clip.min.element_wise_max(&global_frame.min);
                ctx.clip.max = ctx.clip.max.element_wise_min(&global_frame.max);
            }
        }

        fn traverse(this: HViewRef<'_>, cb: &mut impl FnMut(HViewRef<'_>), mut ctx: Ctx) {
            let dirty = &this.view.dirty;
            let layout = this.view.layout.borrow();
            dirty.set(dirty.get() | ctx.extra_flags);

            if dirty.get().intersects(ViewDirtyFlags::POSITION_EVENT) {
                update_global_frame(this, &ctx);

                dirty.set(
                    dirty.get()
                        - flags![ViewDirtyFlags::{POSITION_EVENT | DESCENDANT_POSITION_EVENT}],
                );
                cb(this);

                // If we encounter `POSITION_EVENT`, call `position` on every
                // descendant.
                ctx.extra_flags |= ViewDirtyFlags::POSITION_EVENT;
            } else if dirty
                .get()
                .intersects(ViewDirtyFlags::DESCENDANT_POSITION_EVENT)
            {
                dirty.set(dirty.get() - ViewDirtyFlags::DESCENDANT_POSITION_EVENT);
            } else {
                // No subviews have `POSITION_EVENT`, so return early
                return;
            }

            transform_ctx_for_subviews(this, &mut ctx);

            for subview in layout.subviews().iter() {
                traverse(subview.as_ref(), &mut *cb, ctx);
            }
        }

        traverse(
            self,
            &mut |hview| {
                hview.view.listener.borrow().position(wm, hview);
            },
            Ctx {
                clip: box2! {
                    min: [f32::NEG_INFINITY, f32::NEG_INFINITY],
                    max: [f32::INFINITY, f32::INFINITY],
                },
                global_offset: Point2::new(0.0, 0.0),
                extra_flags: ViewDirtyFlags::empty(),
            },
        );
    }

    /// Perform a hit test for the point `p` specified in the window coordinate
    /// space.
    ///
    /// `accept_flag` specifies a flag that causes a view to be taken into
    /// consideration. `deny_flag` specifies a flag that excludes a view and its
    /// subviews.
    pub(super) fn hit_test(
        &self,
        p: Point2<f32>,
        accept_flag: ViewFlags,
        deny_flag: ViewFlags,
    ) -> Option<HView> {
        let flags = self.view.flags.get();

        if flags.intersects(deny_flag) {
            return None;
        }

        let hit_local = self.view.global_frame.get().contains_point(&p);

        if !flags.intersects(ViewFlags::NO_CLIP_HITTEST) && !hit_local {
            return None;
        }

        // Check subviews
        let layout = self.view.layout.borrow();
        for subview in layout.subviews().iter().rev() {
            if let Some(found_view) = subview.as_ref().hit_test(p, accept_flag, deny_flag) {
                return Some(found_view);
            }
        }

        if hit_local && flags.intersects(accept_flag) {
            Some(self.cloned())
        } else {
            None
        }
    }
}

/// The context for [`Layout::arrange`] and [`Layout::size_traits`].
pub struct LayoutCtx<'a> {
    active_view: HViewRef<'a>,
    /// A new layout object, optionally set by `self.set_layout`.
    new_layout: Option<Box<dyn Layout>>,
    #[cfg(debug_assertions)]
    is_arranging: bool,
}

impl<'a> LayoutCtx<'a> {
    /// Get `SizeTraits` for a subview `hview`.
    pub fn subview_size_traits(&self, hview: HViewRef<'_>) -> SizeTraits {
        self.ensure_subview(hview);
        hview.view.size_traits.get()
    }

    /// Set the frame (bounding rectangle) of a subview `hview`.
    ///
    /// This method only can be called from [`Layout::arrange`].
    pub fn set_subview_frame(&mut self, hview: HViewRef<'_>, frame: Box2<f32>) {
        self.ensure_subview(hview);

        #[cfg(debug_assertions)]
        assert!(self.is_arranging);

        // Local position
        if frame.size() != hview.view.frame.get().size() {
            hview.set_dirty_flags(ViewDirtyFlags::SUBVIEWS_FRAME);
            self.active_view
                .set_dirty_flags(ViewDirtyFlags::DESCENDANT_SUBVIEWS_FRAME);
        }

        if frame != hview.view.frame.get() {
            hview.set_dirty_flags(ViewDirtyFlags::POSITION_EVENT);
            self.active_view
                .set_dirty_flags(ViewDirtyFlags::DESCENDANT_POSITION_EVENT);

            trace!("Reframing {:?} with {:?}", hview, frame);
        }

        hview.view.frame.set(frame);

        #[cfg(debug_assertions)]
        hview.view.has_frame.set(true);
    }

    /// Get the frame previously set by `set_subview_frame`.
    ///
    /// This method only can be called from [`Layout::arrange`]. The frame to
    /// retrieve must already be set during the same call to `Layout::arrange`.
    pub fn subview_frame(&self, hview: HViewRef<'_>) -> Box2<f32> {
        self.ensure_subview(hview);

        #[cfg(debug_assertions)]
        assert!(self.is_arranging);

        #[cfg(debug_assertions)]
        assert!(
            hview.view.has_frame.get(),
            "The view {:?} doesn't have a frame set yet",
            hview,
        );

        hview.view.frame.get()
    }

    /// Panic if `hview` is not a subview of the active view and
    /// debug assertions are enabled.
    fn ensure_subview(&self, hview: HViewRef<'_>) {
        debug_assert_eq!(
            *hview.view.superview.borrow(),
            Rc::downgrade(&RcBorrow::upgrade(self.active_view.view)),
            "the view is not a subview"
        );
    }

    /// Replace the active view's layout object, restarting the layout process.
    ///
    /// This operation is only supported by `arrange`.
    ///
    /// If this method is called, the layout attempt of the active view is
    /// considered invalid. Thus, setting the frames of subviews is no longer
    /// necessary.
    pub fn set_layout(&mut self, layout: impl Into<Box<dyn Layout>>) {
        self.new_layout = Some(layout.into());
    }
}
