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
    foundation::{NSSize, NSString, NSPoint},
    quartzcore::transaction,
};
use objc::{
    msg_send,
    runtime::{BOOL, YES},
    sel, sel_impl,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::super::{
    iface::{self, WM as _},
    WndAttrs,
};
use super::{utils::with_autorelease_pool, HLayer, IdRef, WM, drawutils::point2_from_ns_point};

#[derive(Debug, Clone)]
pub struct HWnd {
    /// `TCWWindowController`
    ctrler: IdRef,
}

struct WndState {
    listener: RefCell<Box<dyn iface::WndListener<WM>>>,
    layer: Cell<Option<HLayer>>,
    hwnd: HWnd,
}

impl HWnd {
    /// Must be called from a main thread.
    pub(super) unsafe fn new(attrs: WndAttrs<'_>) -> Self {
        with_autorelease_pool(|| {
            extern "C" {
                /// Return `[TCWWindowController class]`.
                fn tcw_wnd_ctrler_cls() -> id;
            }

            let ctrler: id = msg_send![tcw_wnd_ctrler_cls(), alloc];
            let ctrler = IdRef::new(msg_send![ctrler, init]).non_nil().unwrap();

            // Create a handle
            let this = HWnd { ctrler };

            // Create `WndState`
            let state = Rc::new(WndState {
                listener: RefCell::new(Box::new(())),
                layer: Cell::new(None),
                hwnd: this.clone(),
            });

            // Attach `WndState`
            let () = msg_send![*this.ctrler, setListenerUserData: Rc::into_raw(state)];

            this.set_attrs(attrs);
            let () = msg_send![*this.ctrler, center];

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

    /// Must be called from a main thread.
    pub(super) unsafe fn set_attrs(&self, attrs: WndAttrs<'_>) {
        let state = self.state();

        // Call `setFlags` before `setContentSize` to make sure the window
        // properly sized based on the target window style masks
        if let Some(value) = attrs.flags {
            let () = msg_send![*self.ctrler, setFlags: value.bits()];
        }

        if let Some(value) = attrs.size {
            let size = NSSize::new(value[0] as _, value[1] as _);
            let () = msg_send![*self.ctrler, setContentSize: size];
        }

        if let Some(value) = attrs.min_size {
            let min_size = NSSize::new(value[0] as _, value[1] as _);
            let () = msg_send![*self.ctrler, setContentMinSize: min_size];
        }

        if let Some(value) = attrs.max_size {
            let max_size = NSSize::new(value[0] as _, value[1] as _);
            let () = msg_send![*self.ctrler, setContentMaxSize: max_size];
        }

        if let Some(value) = &attrs.caption {
            let title = IdRef::new(NSString::alloc(nil).init_str(&**value));
            let () = msg_send![*self.ctrler, setTitle:*title];
        }

        match attrs.visible {
            Some(true) => {
                let () = msg_send![*self.ctrler, makeKeyAndOrderFront];
            }
            Some(false) => {
                let () = msg_send![*self.ctrler, orderOut];
            }
            None => {}
        }

        if let Some(value) = attrs.listener {
            state.listener.replace(value);
        }

        if let Some(value) = attrs.layer {
            let layer = if let Some(hlayer) = value {
                hlayer.ca_layer(WM::global_unchecked())
            } else {
                nil
            };
            let () = msg_send![*self.ctrler, setLayer: layer];
            state.layer.set(value);
        }
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn remove(&self) {
        with_autorelease_pool(|| {
            let () = msg_send![*self.ctrler, close];
        });
    }

    pub(super) fn update(&self, wm: WM) {
        if let Some(layer) = self.state().layer.get() {
            with_autorelease_pool(|| {
                transaction::begin();
                transaction::set_animation_duration(0.0);
                layer.flush(wm);
                transaction::commit();
            });
        }
    }

    pub(super) fn get_size(&self, _: WM) -> [u32; 2] {
        let size: NSSize = unsafe { msg_send![*self.ctrler, contentSize] };
        [size.width as u32, size.height as u32]
    }

    pub(super) fn get_dpi_scale(&self, _: WM) -> f32 {
        unsafe { msg_send![*self.ctrler, dpiScale] }
    }
}

// These functions are called by `TCWWindowController`
type TCWListenerUserData = *const WndState;

unsafe fn method_impl<T>(
    ud: TCWListenerUserData,
    f: impl FnOnce(WM, &WndState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = WM::global_unchecked();
    Some(f(wm, &*ud))
}

// TODO: catch panics

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_should_close(ud: TCWListenerUserData) -> BOOL {
    method_impl(ud, |wm, state| {
        state.listener.borrow().close_requested(wm, &state.hwnd) as _
    })
    .unwrap_or(YES)
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_close(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().close(wm, &state.hwnd);

        // Detach the listener from the controller
        let () = msg_send![*state.hwnd.ctrler, setListenerUserData: nil];
    });

    if !ud.is_null() {
        Rc::from_raw(ud);
    }
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_resize(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().resize(wm, &state.hwnd);
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_dpi_scale_changed(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().dpi_scale_changed(wm, &state.hwnd);
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_motion(ud: TCWListenerUserData, loc: NSPoint)  {
    method_impl(ud, |wm, state| {
        state.listener.borrow().mouse_motion(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap());
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_leave(ud: TCWListenerUserData)  {
    method_impl(ud, |wm, state| {
        state.listener.borrow().mouse_leave(wm, &state.hwnd);
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_drag(ud: TCWListenerUserData, loc: NSPoint, button: u8) -> TCWMouseDragListenerUserData {
    method_impl(ud, |wm, state| {
        let listener = state.listener.borrow().mouse_drag(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap(), button);

        let state = DragState {
            listener,
            hwnd: state.hwnd.clone(),
        };

        Box::into_raw(Box::new(state)) as *const _
    }).unwrap_or(std::ptr::null())
}

type TCWMouseDragListenerUserData = *const DragState;

struct DragState {
    listener: Box<dyn iface::MouseDragListener<WM>>,
    hwnd: HWnd,
}

unsafe fn drag_method_impl<T>(
    ud: TCWMouseDragListenerUserData,
    f: impl FnOnce(WM, &DragState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = WM::global_unchecked();
    Some(f(wm, &*ud))
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_release(ud: TCWMouseDragListenerUserData)  {
    if !ud.is_null() {
        Box::from_raw(ud as *mut DragState);
    }
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_cancel(ud: TCWMouseDragListenerUserData)  {
    drag_method_impl(ud, |wm, state| {
        state.listener.cancel(wm, &state.hwnd);
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_motion(ud: TCWMouseDragListenerUserData, loc :NSPoint)  {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_motion(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap());
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_down(ud: TCWMouseDragListenerUserData, loc :NSPoint, button: u8)  {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_down(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap(), button);
    });
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_up(ud: TCWMouseDragListenerUserData, loc :NSPoint, button: u8)  {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_up(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap(), button);
    });
}
