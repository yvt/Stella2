//! The backend for macOS/Cocoa.
use cocoa::{
    appkit,
    appkit::{NSApplication, NSApplicationActivationPolicy},
};
use objc::{msg_send, sel, sel_impl};
use std::marker::PhantomData;

use super::{traits, types};

mod utils;
mod window;
use self::utils::{ensure_main_thread, IdRef};
pub use self::window::HWnd;

/// Provides an access to the window system.
///
/// `WM` is only accessible by the application's main thread.
pub struct WM {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

impl WM {
    pub fn global() -> &'static WM {
        ensure_main_thread();
        unsafe { Self::global_unchecked() }
    }

    pub unsafe fn global_unchecked() -> &'static WM {
        &WM {
            _no_send_sync: PhantomData,
        }
    }
}

impl traits::WM for WM {
    type HWnd = HWnd;

    fn enter_main_loop(&self) {
        unsafe {
            let app = appkit::NSApp();
            app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
            );
            app.finishLaunching();
            app.run();
        }
    }

    fn terminate(&self) {
        unsafe {
            let app = appkit::NSApp();
            let () = msg_send![app, terminate];
        }
    }

    fn new_wnd(&self, attrs: &types::WndAttrs<Self, &str>) -> Self::HWnd {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { HWnd::new(attrs) }
    }

    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &types::WndAttrs<Self, &str>) {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { window.set_attrs(attrs) }
    }

    fn remove_wnd(&self, window: &Self::HWnd) {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { window.remove() }
    }
}
