use arrayvec::ArrayVec;
use cgmath::{Point2, Vector2};
use log::{trace, warn};
use std::fmt;
use std::rc::{Rc, Weak};

use super::{CursorShape, HView, HViewRef, HWnd, ScrollDelta, ViewFlags, Wnd};
use crate::{pal, pal::Wm};

/// Mouse event handlers for mouse drag gestures.
///
/// A `MouseDragListener` object lives until one of the following events occur:
///
///  - `mouse_up` is called and there are no currently pressed buttons.
///  - `cancel` is called.
///
/// Positions are represented in the containing window's coordinate space.
pub trait MouseDragListener {
    /// The mouse pointer has moved inside a window when at least one of the
    /// mouse buttons are pressed.
    fn mouse_motion(&self, _: Wm, _: HViewRef<'_>, _loc: Point2<f32>) {}

    /// A mouse button was pressed inside a window.
    fn mouse_down(&self, _: Wm, _: HViewRef<'_>, _loc: Point2<f32>, _button: u8) {}

    /// A mouse button was released inside a window.
    ///
    /// When all mouse buttons are released, a reference to `MouseDragListener`
    /// is destroyed.
    /// A brand new `MouseDragListener` will be created via
    /// [`WndListener::mouse_drag`] next time a mouse button is pressed.
    ///
    /// [`WndListener::mouse_drag`]: crate::pal::iface::WndListener::mouse_drag
    fn mouse_up(&self, _: Wm, _: HViewRef<'_>, _loc: Point2<f32>, _button: u8) {}

    /// A mouse drag gesture was cancelled.
    fn cancel(&self, _: Wm, _: HViewRef<'_>) {}
}

/// A default implementation of [`MouseDragListener`].
impl MouseDragListener for () {}

/// Event handlers for scroll gestures.
///
/// A `ScrollListener` object lives until one of the following events occur:
///
///  - `end` is called.
///  - `cancel` is called.
///
pub trait ScrollListener {
    /// The mouse's scroll wheel was moved.
    ///
    /// `velocity` represents the estimated current scroll speed, which is
    /// useful for implementing the rubber-band effect during intertia scrolling.
    fn motion(&self, _: Wm, _: HViewRef<'_>, _delta: &ScrollDelta, _velocity: Vector2<f32>) {}

    /// Mark the start of a momentum phase (also known as *inertia scrolling*).
    ///
    /// After calling this method, the system will keep generating `motion`
    /// events with dissipating delta values.
    fn start_momentum_phase(&self, _: Wm, _: HViewRef<'_>) {}

    /// The gesture was completed.
    fn end(&self, _: Wm, _: HViewRef<'_>) {}

    /// The gesture was cancelled.
    fn cancel(&self, _: Wm, _: HViewRef<'_>) {}
}

/// A default implementation of [`ScrollListener`].
impl ScrollListener for () {}

#[derive(Debug)]
pub(super) struct WndMouseState {
    drag_gestures: Option<Rc<DragGesture>>,
    scroll_gestures: Option<Rc<ScrollGesture>>,
    hover_view: Option<HView>,
}

impl WndMouseState {
    pub fn new() -> Self {
        Self {
            drag_gestures: None,
            scroll_gestures: None,
            hover_view: None,
        }
    }
}

/// Represents an active mouse drag gesture.
struct DragGesture {
    view: HView,
    listener: Box<dyn MouseDragListener>,
}

impl fmt::Debug for DragGesture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DragGesture")
            .field("view", &self.view)
            .field("listener", &((&*self.listener) as *const _))
            .finish()
    }
}

/// Represents an active scroll gesture.
struct ScrollGesture {
    view: HView,
    listener: Box<dyn ScrollListener>,
}

impl fmt::Debug for ScrollGesture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ScrollGesture")
            .field("view", &self.view)
            .field("listener", &((&*self.listener) as *const _))
            .finish()
    }
}

impl HWnd {
    /// The core implementation of `pal::WndListener::mouse_motion` and
    /// `pal::WndListener::mouse_leave`.
    pub(super) fn handle_mouse_motion(&self, loc: Option<Point2<f32>>) {
        let mut st = self.wnd.mouse_state.borrow_mut();

        let new_hover_view = loc.and_then(|loc| {
            let content_view = self.wnd.content_view.borrow();
            content_view.as_ref().unwrap().as_ref().hit_test(
                loc,
                ViewFlags::ACCEPT_MOUSE_OVER,
                ViewFlags::DENY_MOUSE,
            )
        });

        if new_hover_view == st.hover_view {
            return;
        }

        let mut path1 = ArrayVec::new();
        let mut path2 = ArrayVec::new();

        HViewRef::get_path_if_some(st.hover_view.as_ref().map(|hv| hv.as_ref()), &mut path1);
        HViewRef::get_path_if_some(new_hover_view.as_ref().map(|hv| hv.as_ref()), &mut path2);

        // Find the lowest common ancestor
        use itertools::izip;
        let lca_depth = izip!(path1.iter().rev(), path2.iter().rev())
            .take_while(|(v1, v2)| v1 == v2)
            .count();

        debug_assert!(if lca_depth == 0 {
            true
        } else {
            path1[path1.len() - lca_depth] == path2[path2.len() - lca_depth]
        });

        // Call the handlers
        if let Some(hview) = &st.hover_view {
            hview
                .view
                .listener
                .borrow()
                .mouse_out(self.wnd.wm, hview.as_ref());
        }
        for hview in path1[..path1.len() - lca_depth].iter() {
            hview
                .view
                .listener
                .borrow()
                .mouse_leave(self.wnd.wm, hview.as_ref());
        }
        for hview in path2[..path2.len() - lca_depth].iter().rev() {
            hview
                .view
                .listener
                .borrow()
                .mouse_enter(self.wnd.wm, hview.as_ref());
        }
        if let Some(hview) = &new_hover_view {
            hview
                .view
                .listener
                .borrow()
                .mouse_over(self.wnd.wm, hview.as_ref());
        }

        st.hover_view = new_hover_view;

        // Update the cursor shape of the window
        let cursor_shape = path2
            .iter()
            .filter_map(|hview| hview.view.cursor_shape.get())
            .last()
            .unwrap_or_default();
        self.wnd.set_cursor_shape(cursor_shape);
    }

    /// The core implementation of `pal::WndListener::nc_hit_test`.
    #[inline]
    pub(super) fn handle_nc_hit_test(&self, loc: Point2<f32>) -> pal::NcHit {
        let hit_view = {
            let content_view = self.wnd.content_view.borrow();
            content_view.as_ref().unwrap().as_ref().hit_test(
                loc,
                ViewFlags::ACCEPT_MOUSE_DRAG,
                ViewFlags::DENY_MOUSE,
            )
        };

        // If the hit testing returns a view with `DRAG_AREA`, return
        // `NcHit::Grab`.
        if let Some(view) = hit_view {
            if view.view.flags.get().contains(ViewFlags::DRAG_AREA) {
                return pal::NcHit::Grab;
            }
        }

        pal::NcHit::Client
    }

    /// The core implementation of `pal::WndListener::mouse_drag`.
    #[inline]
    pub(super) fn handle_mouse_drag(
        &self,
        loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn pal::iface::MouseDragListener<pal::Wm>> {
        let mut st = self.wnd.mouse_state.borrow_mut();

        if st.drag_gestures.is_some() {
            // Can't have more than one active drag gesture
            // (Is that even possible?)

            warn!(
                "{:?}: Rejecting mouse click at {:?} (button = {:?}) because \
                 there already is an active drag gesture",
                self, loc, button
            );

            return Box::new(());
        }

        let hit_view = {
            let content_view = self.wnd.content_view.borrow();
            content_view.as_ref().unwrap().as_ref().hit_test(
                loc,
                ViewFlags::ACCEPT_MOUSE_DRAG,
                ViewFlags::DENY_MOUSE,
            )
        };

        trace!(
            "{:?}: Mouse click at {:?} (button = {:?}) is handled by {:?}",
            self,
            loc,
            button,
            hit_view
        );

        if let Some(hit_view) = hit_view {
            if !(hit_view.view.flags.get()).contains(ViewFlags::NO_FOCUS_ON_CLICK) {
                if hit_view.as_ref().has_strong_focus_policy() {
                    // Focus the view (if it or its superview accepts a keyboard focus)
                    hit_view.focus();
                } else {
                    // If the currently focused view doesn't follow the strong
                    // focus policy, steal the focus.
                    let should_steal =
                        if let Some(focused_view) = &*self.as_ref().borrow_focused_view() {
                            !(focused_view.view.flags.get()).contains(ViewFlags::STRONG_FOCUS)
                        } else {
                            false
                        };
                    if should_steal {
                        self.set_focused_view(None);
                    }
                }
            }

            // Call the view's drag event handler
            let view_drag_listener = {
                let listener = hit_view.view.listener.borrow();
                listener.mouse_drag(self.wnd.wm, hit_view.as_ref(), loc, button)
            };

            // Remember the gesture
            st.drag_gestures = Some(Rc::new(DragGesture {
                view: hit_view,
                listener: view_drag_listener,
            }));

            // Return `dyn pal::iface::MouseDragListener`
            Box::new(PalDragListener {
                wnd: Rc::downgrade(&self.wnd),
            })
        } else {
            Box::new(())
        }
    }

    /// The core implementation of `pal::WndListener::scroll_motion`.
    pub(super) fn handle_scroll_motion(&self, loc: Point2<f32>, delta: &ScrollDelta) {
        if self.wnd.mouse_state.borrow().scroll_gestures.is_some() {
            // PAL broke the contract
            warn!(
                "{:?}: Rejecting scroll motion event at {:?} (delta = {:?}) because \
                 there already is an active scroll gesture",
                self, loc, delta
            );
            return;
        }

        let hit_view = {
            let content_view = self.wnd.content_view.borrow();
            content_view.as_ref().unwrap().as_ref().hit_test(
                loc,
                ViewFlags::ACCEPT_SCROLL,
                ViewFlags::DENY_MOUSE,
            )
        };

        trace!(
            "{:?}: Scroll motion at {:?} (delta = {:?}) is handled by {:?}",
            self,
            loc,
            delta,
            hit_view
        );

        if let Some(hit_view) = hit_view {
            // Call the view's drag event handler
            let listener = hit_view.view.listener.borrow();
            listener.scroll_motion(self.wnd.wm, hit_view.as_ref(), loc, delta);
        }
    }

    /// The core implementation of `pal::WndListener::scroll_gesture`.
    pub(super) fn handle_scroll_gesture(
        &self,
        loc: Point2<f32>,
    ) -> Box<dyn pal::iface::ScrollListener<pal::Wm>> {
        let mut st = self.wnd.mouse_state.borrow_mut();

        if st.scroll_gestures.is_some() {
            // Can't have more than one active scroll gesture
            // (Is that even possible?)

            warn!(
                "{:?}: Rejecting the new scroll gesture at {:?} because \
                 there already is an active scroll gesture",
                self, loc
            );

            return Box::new(());
        }

        let hit_view = {
            let content_view = self.wnd.content_view.borrow();
            content_view.as_ref().unwrap().as_ref().hit_test(
                loc,
                ViewFlags::ACCEPT_SCROLL,
                ViewFlags::DENY_MOUSE,
            )
        };

        trace!(
            "{:?}: Scroll gesture at {:?} is handled by {:?}",
            self,
            loc,
            hit_view
        );

        if let Some(hit_view) = hit_view {
            // Call the view's drag event handler
            let view_scr_listener = {
                let listener = hit_view.view.listener.borrow();
                listener.scroll_gesture(self.wnd.wm, hit_view.as_ref(), loc)
            };

            // Remember the gesture
            st.scroll_gestures = Some(Rc::new(ScrollGesture {
                view: hit_view,
                listener: view_scr_listener,
            }));

            // Return `dyn pal::iface::MouseDragListener`
            Box::new(PalScrollListener {
                wnd: Rc::downgrade(&self.wnd),
            })
        } else {
            Box::new(())
        }
    }
}

impl HViewRef<'_> {
    /// Cancel all active mouse gestures for the specified view and its
    /// subviews.
    pub(super) fn cancel_mouse_gestures_of_subviews(self, wnd: &Wnd) {
        let cancelled_drag = wnd
            .mouse_state
            .borrow_mut()
            .cancel_drag_gestures(self, true);

        if let Some(drag) = cancelled_drag {
            drag.listener.cancel(wnd.wm, drag.view.as_ref());
        }
    }

    /// Cancel active mouse drag gestures for the specified view (but not
    /// subviews).
    pub(super) fn cancel_mouse_drag_gestures(self, wnd: &Wnd) {
        let cancelled_drag = wnd
            .mouse_state
            .borrow_mut()
            .cancel_drag_gestures(self, false);

        if let Some(drag) = cancelled_drag {
            drag.listener.cancel(wnd.wm, drag.view.as_ref());
        }
    }

    /// Recalculate the current cursor shape if `self` is relevant to the
    /// calculation.
    pub(super) fn update_cursor(self, wnd: &Wnd) {
        let st = wnd.mouse_state.borrow();

        if let Some(view) = &st.hover_view {
            if view.as_ref().is_improper_subview_of(self) {
                // Update the cursor shape of the window
                let mut cursor_shape = CursorShape::default();
                view.as_ref().for_each_ancestor(|hview| {
                    if let Some(shape) = hview.view.cursor_shape.get() {
                        cursor_shape = shape;
                    }
                });
                wnd.set_cursor_shape(cursor_shape);
            }
        }
    }
}

impl WndMouseState {
    /// Cancel drag gestures for `view` (if any).
    ///
    /// If `subview` is `true`, the subviews of `view` are also affected.
    ///
    /// Returns `Some(drag)` if the drag gesture `drag` is cancelled. The caller
    /// should call `drag.listener.cancel` after unborrowing `Wnd::mouse_state`.
    fn cancel_drag_gestures(
        &mut self,
        view: HViewRef<'_>,
        subview: bool,
    ) -> Option<Rc<DragGesture>> {
        let cancel_drag;
        if let Some(drag) = &self.drag_gestures {
            if subview {
                cancel_drag = drag.view.as_ref().is_improper_subview_of(view);
            } else {
                cancel_drag = drag.view.as_ref() == view;
            }
        } else {
            cancel_drag = false;
        }

        // Technically, `PalDragListener` don't know that the gesture was
        // cancelled and might send events to a wrong view in the future,
        // but that shouldn't be an issue in reality...

        if cancel_drag {
            Some(self.drag_gestures.take().unwrap())
        } else {
            None
        }
    }
}

/// Implements `pal::iface::MouseDragListener`.
struct PalDragListener {
    wnd: Weak<Wnd>,
}

impl PalDragListener {
    /// Get `HWnd` if the underlying object is still alive.
    fn hwnd(&self) -> Option<HWnd> {
        self.wnd.upgrade().map(|wnd| HWnd { wnd })
    }

    fn with_drag_gesture(&self, cb: impl FnOnce(&DragGesture)) {
        if let Some(hwnd) = self.hwnd() {
            let drag = hwnd.wnd.mouse_state.borrow().drag_gestures.clone();
            // Make sure `mouse_state` is unborrowed before calling
            // event handlers
            if let Some(drag) = &drag {
                cb(drag);
            }
        }
    }
}

impl Drop for PalDragListener {
    fn drop(&mut self) {
        if let Some(hwnd) = self.hwnd() {
            trace!("{:?}: Mouse drag gesture ended", hwnd);

            let drag = hwnd.wnd.mouse_state.borrow_mut().drag_gestures.take();
            drop(drag);
        } else {
            trace!("Mouse drag gesture ended, but the owner is gone");
        }
    }
}

/// Forwards events from `pal::iface::MouseDragListener` to
/// `uicore::MouseDragListener`.
impl pal::iface::MouseDragListener<pal::Wm> for PalDragListener {
    fn mouse_motion(&self, wm: Wm, _: &pal::HWnd, loc: Point2<f32>) {
        self.with_drag_gesture(|drag| {
            drag.listener.mouse_motion(wm, drag.view.as_ref(), loc);
        })
    }
    fn mouse_down(&self, wm: Wm, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        self.with_drag_gesture(|drag| {
            drag.listener
                .mouse_down(wm, drag.view.as_ref(), loc, button);
        })
    }
    fn mouse_up(&self, wm: Wm, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        self.with_drag_gesture(|drag| {
            drag.listener.mouse_up(wm, drag.view.as_ref(), loc, button);
        })
    }
    fn cancel(&self, wm: Wm, _: &pal::HWnd) {
        self.with_drag_gesture(|drag| {
            drag.listener.cancel(wm, drag.view.as_ref());
        })
    }
}

/// Implements `pal::iface::ScrollListener`.
struct PalScrollListener {
    wnd: Weak<Wnd>,
}

impl PalScrollListener {
    /// Get `HWnd` if the underlying object is still alive.
    fn hwnd(&self) -> Option<HWnd> {
        self.wnd.upgrade().map(|wnd| HWnd { wnd })
    }

    fn with_scroll_gesture(&self, cb: impl FnOnce(&ScrollGesture)) {
        if let Some(hwnd) = self.hwnd() {
            let gesture = hwnd.wnd.mouse_state.borrow().scroll_gestures.clone();
            // Make sure `mouse_state` is unborrowed before calling
            // event handlers
            if let Some(gesture) = &gesture {
                cb(gesture);
            }
        }
    }
}

impl Drop for PalScrollListener {
    fn drop(&mut self) {
        if let Some(hwnd) = self.hwnd() {
            trace!("{:?}: Scroll gesture ended", hwnd);

            let gesture = hwnd.wnd.mouse_state.borrow_mut().scroll_gestures.take();
            drop(gesture);
        } else {
            trace!("Scroll gesture ended, but the owner is gone");
        }
    }
}

/// Forwards events from `pal::iface::ScrollListener` to
/// `uicore::ScrollListener`.
impl pal::iface::ScrollListener<pal::Wm> for PalScrollListener {
    fn motion(&self, wm: Wm, _: &pal::HWnd, delta: &pal::ScrollDelta, velocity: Vector2<f32>) {
        self.with_scroll_gesture(|gesture| {
            gesture
                .listener
                .motion(wm, gesture.view.as_ref(), delta, velocity);
        })
    }
    fn start_momentum_phase(&self, wm: Wm, _: &pal::HWnd) {
        self.with_scroll_gesture(|gesture| {
            gesture
                .listener
                .start_momentum_phase(wm, gesture.view.as_ref());
        })
    }
    fn end(&self, wm: Wm, _: &pal::HWnd) {
        self.with_scroll_gesture(|gesture| {
            gesture.listener.end(wm, gesture.view.as_ref());
        })
    }
    fn cancel(&self, wm: Wm, _: &pal::HWnd) {
        self.with_scroll_gesture(|gesture| {
            gesture.listener.cancel(wm, gesture.view.as_ref());
        })
    }
}
