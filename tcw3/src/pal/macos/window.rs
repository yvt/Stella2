//! The window type implementation for macOS.
//!
//! The relationship of objects are shown below:
//!
//! ```text
//!      (this cycle is severed at this point
//!      when the window is closed)
//!   NSWindow ------------> TCW3WindowDelegate
//!      ^                          |
//!      |                          v
//!    HWnd <------------------- WndState
//!      ^  (HWnd == id<NSWindow>)  |
//!      |                          |
//!      '----- WndListener <-------'
//! ```
use cocoa::{
    appkit,
    appkit::{NSWindow, NSWindowStyleMask},
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize, NSString},
};
use lazy_static::lazy_static;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel, BOOL, NO, YES},
    sel, sel_impl,
};
use owning_ref::OwningRef;
use std::{cell::RefCell, os::raw::c_void, rc::Rc};

use super::super::{traits, types};
use super::{utils::with_autorelease_pool, IdRef, WM};

#[derive(Clone)]
pub struct HWnd {
    window: IdRef,
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
            let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0));
            let masks = NSWindowStyleMask::NSClosableWindowMask
                | NSWindowStyleMask::NSMiniaturizableWindowMask
                | NSWindowStyleMask::NSResizableWindowMask
                | NSWindowStyleMask::NSTitledWindowMask;

            let window_id = NSWindow::alloc(nil);
            let window = IdRef::new(window_id.initWithContentRect_styleMask_backing_defer_(
                frame,
                masks,
                appkit::NSBackingStoreBuffered,
                NO,
            ))
            .non_nil()
            .unwrap();

            window.setReleasedWhenClosed_(NO);

            // Create a handle
            let this = HWnd { window };

            // Create `WndState`
            let state = Box::new(WndState {
                listener: RefCell::new(None),
                hwnd: this.clone(),
            });

            // Set the window delegate
            let delegate = wnd_delegate::new(state);
            let () = msg_send![*this.window, setDelegate:*delegate];
            // `NSWindow.delegate` is `@property(weak)`, so make it behave like
            // strong by incrementing the delegate's ref count
            // (This causes a false reading on Xcode's memory memory graph,
            // though...)
            std::mem::forget(delegate);

            this.set_attrs(attrs);
            this.window.center();

            this
        })
    }

    /// Return a smart pointer to the `WndState` associated with this `HWnd`.
    fn state(&self) -> impl std::ops::Deref<Target = WndState> {
        unsafe {
            let delegate = IdRef::retain(msg_send![*self.window, delegate])
                .non_nil()
                .unwrap();

            OwningRef::new(delegate).map(|delegate| wnd_delegate::get_state(&**delegate))
        }
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn set_attrs(&self, attrs: &types::WndAttrs<WM, &str>) {
        let state = self.state();

        if let Some(value) = attrs.size {
            self.window
                .setContentSize_(NSSize::new(value[0] as _, value[1] as _));
        }

        if let Some(value) = attrs.caption {
            let title = IdRef::new(NSString::alloc(nil).init_str(value));
            self.window.setTitle_(*title);
        }

        match attrs.visible {
            Some(true) => {
                self.window.makeKeyAndOrderFront_(nil);
            }
            Some(false) => {
                self.window.orderOut_(nil);
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
            let () = msg_send![*self.window, close];

            let delegate: id = msg_send![*self.window, delegate];
            let () = msg_send![*self.window, setDelegate: nil];
            let () = msg_send![delegate, release];
        });
    }
}

mod wnd_delegate {
    use super::*;

    /// Instantiate `TCW3WindowDelegate`.
    pub(super) fn new(state: Box<WndState>) -> IdRef {
        unsafe {
            let delegate = IdRef::new(msg_send![*WND_DELEGATE_CLASS, new]);
            (&mut **delegate).set_ivar("state", Box::into_raw(state) as *mut c_void);
            delegate
        }
    }

    /// - Must be called from a main thread.
    /// - `this` is a valid pointer to an instance of `TCW3WindowDelegate`.
    pub(super) unsafe fn get_state(this: &Object) -> &WndState {
        let state: *mut c_void = *(*this).get_ivar("state");
        &*(state as *const WndState)
    }

    unsafe fn method_impl<T>(this: &Object, f: impl FnOnce(&WM, &WndState) -> T) -> T {
        let wm = unsafe { WM::global_unchecked() };
        f(wm, get_state(this))
    }

    extern "C" fn window_should_close(this: &Object, _: Sel, _: id) -> BOOL {
        unsafe {
            method_impl(this, |wm, state| {
                if let Some(ref listener) = *state.listener.borrow() {
                    listener.close_requested(&wm, &state.hwnd) as _
                } else {
                    YES
                }
            })
        }
    }

    extern "C" fn window_will_close(this: &Object, _: Sel, _: id) {
        unsafe {
            method_impl(this, |wm, state| {
                if let Some(ref listener) = *state.listener.borrow() {
                    listener.close(&wm, &state.hwnd)
                }

                // Let's hope that the parent function maintains a strong
                // reference to this delegate while calling this method
                let () = msg_send![*state.hwnd.window, setDelegate: nil];
            })
        }
    }

    extern "C" fn dealloc(this: &Object, _: Sel) {
        unsafe { Box::from_raw(get_state(this) as *const _ as *mut WndState) };
    }

    lazy_static! {
        static ref WND_DELEGATE_CLASS: &'static Class = unsafe {
            let superclass = class!(NSObject);
            let mut decl = ClassDecl::new("TCW3WindowDelegate", superclass).unwrap();

            decl.add_method(
                sel!(windowShouldClose:),
                window_should_close as extern "C" fn(&_, _, _) -> _,
            );

            decl.add_method(
                sel!(windowWillClose:),
                window_will_close as extern "C" fn(&_, _, _) -> _,
            );

            decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&_, _) -> _);

            decl.add_ivar::<*mut c_void>("state");

            decl.register()
        };
    }
}
