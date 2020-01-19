//! The window type implementation for macOS.
//!
//! The relationship of objects are shown below:
//!
//! ```text
//!  TCWWindowController -------> NSWindow
//!    ^             | ^             |
//!    |             | '-------------' (weak)
//!    |             |
//!    |             | <--- (this cycle is severed here
//!    |             |       when the window is closed)
//!    |             v
//!  HWnd <------ WndState  (HWnd == id<TCWWindowController>)
//!    ^                |
//!    |                |
//!    '- WndListener <-'
//! ```
use cocoa::{
    base::{id, nil},
    foundation::{NSPoint, NSSize, NSString},
};
use objc::{
    msg_send,
    runtime::{BOOL, NO},
    sel, sel_impl,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::super::iface::{self, Wm as _};
use super::{
    drawutils::point2_from_ns_point, utils::with_autorelease_pool, HLayer, IdRef, Wm, WndAttrs,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct HWnd {
    /// `TCWWindowController`
    ctrler: IdRef,
}

struct WndState {
    listener: RefCell<Box<dyn iface::WndListener<Wm>>>,
    layer: Cell<Option<HLayer>>,
    hwnd: HWnd,
}

impl HWnd {
    pub(super) fn new(wm: Wm, attrs: WndAttrs<'_>) -> Self {
        with_autorelease_pool(|| {
            extern "C" {
                /// Return `[TCWWindowController class]`.
                fn tcw_wnd_ctrler_cls() -> id;
            }

            let ctrler: id = unsafe { msg_send![tcw_wnd_ctrler_cls(), alloc] };
            let ctrler = IdRef::new(unsafe { msg_send![ctrler, init] })
                .non_nil()
                .unwrap();

            // Create a handle
            let this = HWnd { ctrler };

            // Create `WndState`
            let state = Rc::new(WndState {
                listener: RefCell::new(Box::new(())),
                layer: Cell::new(None),
                hwnd: this.clone(),
            });

            // Attach `WndState`
            let () = unsafe { msg_send![*this.ctrler, setListenerUserData: Rc::into_raw(state)] };

            this.set_attrs(wm, attrs);
            let () = unsafe { msg_send![*this.ctrler, center] };

            this
        })
    }

    /// Return a smart pointer to the `WndState` associated with this `HWnd`.
    fn state(&self) -> Rc<WndState> {
        unsafe {
            let ud: *const WndState = msg_send![*self.ctrler, listenerUserData];
            let rc = Rc::from_raw(ud);
            Rc::into_raw(Rc::clone(&rc));
            rc
        }
    }

    pub(super) fn set_attrs(&self, wm: Wm, attrs: WndAttrs<'_>) {
        let state = self.state();

        // Call `setFlags` before `setContentSize` to make sure the window
        // properly sized based on the target window style masks
        if let Some(value) = attrs.flags {
            let () = unsafe { msg_send![*self.ctrler, setFlags: value.bits()] };
        }

        if let Some(value) = attrs.size {
            let size = NSSize::new(value[0] as _, value[1] as _);
            let () = unsafe { msg_send![*self.ctrler, setContentSize: size] };
        }

        if let Some(value) = attrs.min_size {
            let min_size = NSSize::new(value[0] as _, value[1] as _);
            let () = unsafe { msg_send![*self.ctrler, setContentMinSize: min_size] };
        }

        if let Some(value) = attrs.max_size {
            let max_size = NSSize::new(value[0] as _, value[1] as _);
            let () = unsafe { msg_send![*self.ctrler, setContentMaxSize: max_size] };
        }

        if let Some(value) = &attrs.caption {
            let title = IdRef::new(unsafe { NSString::alloc(nil).init_str(&**value) });
            let () = unsafe { msg_send![*self.ctrler, setTitle:*title] };
        }

        match attrs.visible {
            Some(true) => {
                let () = unsafe { msg_send![*self.ctrler, makeKeyAndOrderFront] };
            }
            Some(false) => {
                let () = unsafe { msg_send![*self.ctrler, orderOut] };
            }
            None => {}
        }

        if let Some(value) = attrs.listener {
            state.listener.replace(value);
        }

        if let Some(value) = attrs.layer {
            let layer = if let Some(hlayer) = &value {
                hlayer.ca_layer(wm)
            } else {
                nil
            };
            let () = unsafe { msg_send![*self.ctrler, setLayer: layer] };
            state.layer.set(value);
        }

        if let Some(value) = attrs.cursor_shape {
            let value = value as u32;
            let () = unsafe { msg_send![*self.ctrler, setCursorShape: value] };
        }
    }

    pub(super) fn remove(&self, _: Wm) {
        with_autorelease_pool(|| {
            let () = unsafe { msg_send![*self.ctrler, close] };
        });
    }

    pub(super) fn update(&self, _: Wm) {
        // The system automatically commits any implicit transaction
    }

    pub(super) fn request_update_ready(&self, _wm: Wm) {
        let () = unsafe { msg_send![*self.ctrler, requestUpdateReady] };
    }

    pub(super) fn get_size(&self, _: Wm) -> [u32; 2] {
        let size: NSSize = unsafe { msg_send![*self.ctrler, contentSize] };
        [size.width as u32, size.height as u32]
    }

    pub(super) fn get_dpi_scale(&self, _: Wm) -> f32 {
        unsafe { msg_send![*self.ctrler, dpiScale] }
    }

    pub(super) fn is_focused(&self, _: Wm) -> bool {
        let value: BOOL = unsafe { msg_send![*self.ctrler, isKeyWindow] };
        value != 0
    }
}

// These functions are called by `TCWWindowController`
type TCWListenerUserData = *const WndState;

unsafe fn method_impl<T>(ud: TCWListenerUserData, f: impl FnOnce(Wm, &WndState) -> T) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = Wm::global_unchecked();
    Some(f(wm, &*ud))
}

// TODO: catch panics

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_should_close(ud: TCWListenerUserData) -> BOOL {
    method_impl(ud, |wm, state| {
        state.listener.borrow().close_requested(wm, &state.hwnd);
    });

    NO
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_close(ud: TCWListenerUserData) {
    method_impl(ud, |_wm, state| {
        // Detach the listener from the controller
        let () = msg_send![*state.hwnd.ctrler, setListenerUserData: nil];
    });

    if !ud.is_null() {
        Rc::from_raw(ud);
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_resize(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().resize(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_dpi_scale_changed(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().dpi_scale_changed(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_focus(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().focus(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_update_ready(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().update_ready(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_motion(ud: TCWListenerUserData, loc: NSPoint) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().mouse_motion(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_leave(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().mouse_leave(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_drag(
    ud: TCWListenerUserData,
    loc: NSPoint,
    button: u8,
) -> TCWMouseDragListenerUserData {
    method_impl(ud, |wm, state| {
        let listener = state.listener.borrow().mouse_drag(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            button,
        );

        let state = DragState {
            listener,
            hwnd: state.hwnd.clone(),
        };

        Box::into_raw(Box::new(state)) as *const _
    })
    .unwrap_or(std::ptr::null())
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_scroll_motion(
    ud: TCWListenerUserData,
    loc: NSPoint,
    precise: u8,
    delta_x: f64,
    delta_y: f64,
) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().scroll_motion(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            &iface::ScrollDelta {
                precise: precise != 0,
                delta: [delta_x as f32, delta_y as f32].into(),
            },
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_scroll_gesture(
    ud: TCWListenerUserData,
    loc: NSPoint,
) -> TCWScrollListenerUserData {
    method_impl(ud, |wm, state| {
        let listener = state.listener.borrow().scroll_gesture(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
        );

        let state = ScrollState {
            listener,
            hwnd: state.hwnd.clone(),
        };

        Box::into_raw(Box::new(state)) as *const _
    })
    .unwrap_or(std::ptr::null())
}

type TCWMouseDragListenerUserData = *const DragState;

struct DragState {
    listener: Box<dyn iface::MouseDragListener<Wm>>,
    hwnd: HWnd,
}

unsafe fn drag_method_impl<T>(
    ud: TCWMouseDragListenerUserData,
    f: impl FnOnce(Wm, &DragState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = Wm::global_unchecked();
    Some(f(wm, &*ud))
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_release(ud: TCWMouseDragListenerUserData) {
    if !ud.is_null() {
        Box::from_raw(ud as *mut DragState);
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_cancel(ud: TCWMouseDragListenerUserData) {
    drag_method_impl(ud, |wm, state| {
        state.listener.cancel(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_motion(
    ud: TCWMouseDragListenerUserData,
    loc: NSPoint,
) {
    drag_method_impl(ud, |wm, state| {
        state
            .listener
            .mouse_motion(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap());
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_down(
    ud: TCWMouseDragListenerUserData,
    loc: NSPoint,
    button: u8,
) {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_down(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            button,
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_up(
    ud: TCWMouseDragListenerUserData,
    loc: NSPoint,
    button: u8,
) {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_up(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            button,
        );
    });
}

type TCWScrollListenerUserData = *const ScrollState;

struct ScrollState {
    listener: Box<dyn iface::ScrollListener<Wm>>,
    hwnd: HWnd,
}

unsafe fn scroll_method_impl<T>(
    ud: TCWScrollListenerUserData,
    f: impl FnOnce(Wm, &ScrollState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = Wm::global_unchecked();
    Some(f(wm, &*ud))
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_release(ud: TCWScrollListenerUserData) {
    if !ud.is_null() {
        Box::from_raw(ud as *mut ScrollState);
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_cancel(ud: TCWScrollListenerUserData) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.cancel(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_end(ud: TCWScrollListenerUserData) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.end(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_motion(
    ud: TCWScrollListenerUserData,
    precise: u8,
    delta_x: f64,
    delta_y: f64,
    vel_x: f64,
    vel_y: f64,
) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.motion(
            wm,
            &state.hwnd,
            &iface::ScrollDelta {
                precise: precise != 0,
                delta: [delta_x as f32, delta_y as f32].into(),
            },
            [vel_x as f32, vel_y as f32].into(),
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_start_momentum_phase(ud: TCWScrollListenerUserData) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.start_momentum_phase(wm, &state.hwnd);
    });
}
