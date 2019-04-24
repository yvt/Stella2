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
    runtime::{Class, Object, Sel, BOOL, NO},
    sel, sel_impl,
};
use std::os::raw::c_void;

use super::super::types;
use super::{utils::with_autorelease_pool, IdRef, WM};

#[derive(Clone)]
pub struct HWnd {
    window: IdRef,
}

// FIXME: perhaps it's possible to cause `dealloc` to be called in a worker
//        thread
unsafe impl Send for HWnd {}
unsafe impl Sync for HWnd {}

struct WndState {}

impl HWnd {
    /// Must be called from a main thread.
    pub(super) unsafe fn new(attrs: &types::WndAttrs<WM, &str>) -> Self {
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

        window.center();
        window.setReleasedWhenClosed_(NO);

        // Create a handle
        let this = HWnd { window };

        // Create `WndState`
        let state = Box::new(WndState {});

        // Set the window delegate
        let delegate = wnd_delegate::new(state);
        with_autorelease_pool(|| {
            let () = msg_send![*this.window, setDelegate:*delegate];
        });

        this.set_attrs(attrs);

        this
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn set_attrs(&self, attrs: &types::WndAttrs<WM, &str>) {
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

        // TODO: window listener
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn remove(&self) {
        with_autorelease_pool(|| {
            let () = msg_send![*self.window, close];
            let () = msg_send![*self.window, setDelegate: nil];
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

    unsafe fn method_impl<T>(this: &Object, f: impl FnOnce(&WndState) -> T) -> T {
        f(get_state(this))
    }

    extern "C" fn window_should_close(this: &Object, _: Sel, _: id) -> BOOL {
        unsafe { method_impl(this, |_| NO) }
    }

    extern "C" fn dealloc(this: &Object, _: Sel, _: id) {
        unsafe {
            let state: *mut c_void = *this.get_ivar("state");
            Box::from_raw(state as *mut WndState);
        }
    }

    lazy_static! {
        static ref WND_DELEGATE_CLASS: &'static Class = unsafe {
            let superclass = class!(NSObject);
            let mut decl = ClassDecl::new("TCW3WindowDelegate", superclass).unwrap();

            decl.add_method(
                sel!(windowShouldClose:),
                window_should_close as extern "C" fn(&_, _, _) -> _,
            );

            // FIXME: unregister delegate on close

            decl.add_method(sel!(dealloc:), dealloc as extern "C" fn(&_, _, _) -> _);

            decl.add_ivar::<*mut c_void>("state");

            decl.register()
        };
    }
}
