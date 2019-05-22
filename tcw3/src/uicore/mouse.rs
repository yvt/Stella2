use cgmath::Point2;
use std::fmt;
use std::rc::{Rc, Weak};

use super::{HView, HWnd, ViewFlags, Wnd};
use crate::{pal, pal::WM};

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
    fn mouse_motion(&self, _: WM, _: &HView, _loc: Point2<f32>) {}

    /// A mouse button was pressed inside a window.
    fn mouse_down(&self, _: WM, _: &HView, _loc: Point2<f32>, _button: u8) {}

    /// A mouse button was released inside a window.
    ///
    /// When all mouse buttons are released, a reference to `MouseDragListener`
    /// is destroyed.
    /// A brand new `MouseDragListener` will be created via
    /// [`WndListener::mouse_drag`] next time a mouse button is pressed.
    ///
    /// [`WndListener::mouse_drag`]: crate::pal::iface::WndListener::mouse_drag
    fn mouse_up(&self, _: WM, _: &HView, _loc: Point2<f32>, _button: u8) {}

    /// A mouse drag gesture was cancelled.
    fn cancel(&self, _: WM, _: &HView) {}
}

/// A default implementation of [`MouseDragListener`].
#[derive(Debug, Clone, Copy)]
pub struct DefaultMouseDragListener;

impl MouseDragListener for DefaultMouseDragListener {}

#[derive(Debug)]
pub(super) struct WndMouseState {
    drag_gestures: Option<DragGesture>,
}

impl WndMouseState {
    pub fn new() -> Self {
        Self {
            drag_gestures: None,
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

impl HWnd {
    /// The core implementation of `pal::WndListener::mouse_drag`.
    #[inline]
    pub(super) fn handle_mouse_drag(
        &self,
        loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn pal::iface::MouseDragListener<pal::WM>> {
        let mut st = self.wnd.mouse_state.borrow_mut();

        if st.drag_gestures.is_some() {
            // Can't have more than one active drag gesture
            // (Is that even possible?)
            return Box::new(pal::iface::DefaultMouseDragListener);
        }

        let hit_view = {
            let content_view = self.wnd.content_view.borrow();
            content_view.as_ref().unwrap().hit_test(
                loc,
                ViewFlags::ACCEPT_MOUSE_DRAG,
                ViewFlags::DENY_MOUSE,
            )
        };

        if let Some(hit_view) = hit_view {
            // Call the view's drag event handler
            let view_drag_listener = {
                let listener = hit_view.view.listener.borrow();
                listener.mouse_drag(self.wnd.wm, &hit_view, loc, button)
            };

            // Remember the gesture
            st.drag_gestures = Some(DragGesture {
                view: hit_view,
                listener: view_drag_listener,
            });

            // Return `dyn pal::iface::MouseDragListener`
            Box::new(PalDragListener {
                wnd: Rc::downgrade(&self.wnd),
            })
        } else {
            Box::new(pal::iface::DefaultMouseDragListener)
        }
    }
}

impl HView {
    /// Cancel all active mouse gestures for the specified view and its
    /// subviews.
    pub(super) fn cancel_mouse_gestures_of_subviews(&self, wnd: &Wnd) {
        wnd.mouse_state
            .borrow_mut()
            .cancel_drag_gestures(wnd.wm, self, true);
    }

    /// Cancel active mouse drag gestures for the specified view (but not
    /// subviews).
    pub(super) fn cancel_mouse_drag_gestures(&self, wnd: &Wnd) {
        wnd.mouse_state
            .borrow_mut()
            .cancel_drag_gestures(wnd.wm, self, false);
    }
}

impl WndMouseState {
    /// Cancel drag gestures for `view` (if any).
    ///
    /// If `subview` is `true`, the subviews of `view` are also affected.
    fn cancel_drag_gestures(&mut self, wm: WM, view: &HView, subview: bool) {
        let cancel_drag;
        if let Some(drag) = &self.drag_gestures {
            if subview {
                cancel_drag = drag.view.is_improper_subview_of(view);
            } else {
                cancel_drag = drag.view == *view;
            }
        } else {
            cancel_drag = false;
        }

        // Technically, `PalDragListener` don't know that the gesture was
        // cancelled and might send events to a wrong view in the future,
        // but that shouldn't be an issue in reality...

        if cancel_drag {
            let drag = self.drag_gestures.take().unwrap();
            drag.listener.cancel(wm, &drag.view);
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
            let st = hwnd.wnd.mouse_state.borrow();
            if let Some(drag) = &st.drag_gestures {
                cb(drag);
            }
        }
    }
}

impl Drop for PalDragListener {
    fn drop(&mut self) {
        if let Some(hwnd) = self.hwnd() {
            let mut st = hwnd.wnd.mouse_state.borrow_mut();
            st.drag_gestures = None;
        }
    }
}

/// Forwards events from `pal::iface::MouseDragListener` to
/// `uicore::MouseDragListener`.
impl pal::iface::MouseDragListener<pal::WM> for PalDragListener {
    fn mouse_motion(&self, wm: WM, _: &pal::HWnd, loc: Point2<f32>) {
        self.with_drag_gesture(|drag| {
            drag.listener.mouse_motion(wm, &drag.view, loc);
        })
    }
    fn mouse_down(&self, wm: WM, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        self.with_drag_gesture(|drag| {
            drag.listener.mouse_down(wm, &drag.view, loc, button);
        })
    }
    fn mouse_up(&self, wm: WM, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        self.with_drag_gesture(|drag| {
            drag.listener.mouse_up(wm, &drag.view, loc, button);
        })
    }
    fn cancel(&self, wm: WM, _: &pal::HWnd) {
        self.with_drag_gesture(|drag| {
            drag.listener.cancel(wm, &drag.view);
        })
    }
}
