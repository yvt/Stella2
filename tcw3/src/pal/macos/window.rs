//! The window type implementation for macOS.
//!
//! The relationship of objects are shown below:
//!
//! ```text
//!      (this cycle is severed at this point
//!      when the window is closed)
//!  TCWWindowController -------> NSWindow
//!    ^             | ^             |
//!    |             | '-------------' (weak)
//!    |             v
//!  HWnd <------ WndState  (HWnd == id<TCWWindowController>)
//!    ^              |
//!    |              |
//!    WndListener <--'
//! ```
use cocoa::{
    base::{id, nil},
    foundation::{NSSize, NSString},
};
use objc::{
    msg_send,
    runtime::{BOOL, YES},
    sel, sel_impl,
};
use std::{cell::RefCell, rc::Rc};

use super::super::{traits, types};
use super::{utils::with_autorelease_pool, IdRef, WM};

#[derive(Clone)]
pub struct HWnd {
    /// `TCWWindowController`
    ctrler: IdRef,
}

// FIXME: perhaps it's possible to cause `dealloc` to be called in a worker
//        thread
unsafe impl Send for HWnd {}
unsafe impl Sync for HWnd {}

struct WndState {
    listener: RefCell<Option<Rc<dyn traits::WndListener<WM>>>>,
    hwnd: HWnd,
}

impl HWnd {
    /// Must be called from a main thread.
    pub(super) unsafe fn new(attrs: &types::WndAttrs<WM, &str>) -> Self {
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
                listener: RefCell::new(None),
                hwnd: this.clone(),
            });

            // Attach `WndState`
            msg_send![*this.ctrler, setListenerUserData: Rc::into_raw(state)];

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
    pub(super) unsafe fn set_attrs(&self, attrs: &types::WndAttrs<WM, &str>) {
        let state = self.state();

        if let Some(value) = attrs.size {
            let size = NSSize::new(value[0] as _, value[1] as _);
            let () = msg_send![*self.ctrler, setCotentSize: size];
        }

        if let Some(value) = attrs.caption {
            let title = IdRef::new(NSString::alloc(nil).init_str(value));
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

        if let Some(ref value) = attrs.listener {
            state.listener.replace(value.clone());
        }
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn remove(&self) {
        with_autorelease_pool(|| {
            let () = msg_send![*self.ctrler, close];
        });
    }
}

// These functions are called by `TCWWindowController`
type TCWListenerUserData = *const WndState;

unsafe fn method_impl<T>(
    ud: TCWListenerUserData,
    f: impl FnOnce(&WM, &WndState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = WM::global_unchecked();
    Some(f(wm, &*ud))
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_should_close(ud: TCWListenerUserData) -> BOOL {
    method_impl(ud, |wm, state| {
        if let Some(ref listener) = *state.listener.borrow() {
            listener.close_requested(&wm, &state.hwnd) as _
        } else {
            YES
        }
    })
    .unwrap_or(YES)
}

#[allow(unused_attributes)] // Work-around <https://github.com/rust-lang/rust/issues/60050>
#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_close(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        if let Some(ref listener) = *state.listener.borrow() {
            listener.close(&wm, &state.hwnd)
        }

        // Detach the listener from the controller
        msg_send![*state.hwnd.ctrler, setListenerUserData: nil];
    });

    if !ud.is_null() {
        Rc::from_raw(ud);
    }
}
