//! The backend for macOS, Cocoa, and Core Graphics.
use cocoa::{
    appkit,
    appkit::{NSApplication, NSApplicationActivationPolicy},
    base::nil,
};
use objc::{msg_send, sel, sel_impl};
use std::marker::PhantomData;

use super::{traits, LayerAttrs, WndAttrs};

mod bitmap;
mod drawutils;
mod layer;
mod mtlocal;
mod utils;
mod window;
pub use self::bitmap::{Bitmap, BitmapBuilder};
pub use self::layer::HLayer;
use self::utils::{ensure_main_thread, IdRef};
pub use self::window::HWnd;

/// Provides an access to the window system.
///
/// `WM` is only accessible by the application's main thread. Therefore, the
/// ownership of `&WM` can be used as an evidence that the main thread has the
/// control.
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
    type HLayer = HLayer;
    type Bitmap = Bitmap;

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
            let () = msg_send![app, terminate: nil];
        }
    }

    fn new_wnd(&self, attrs: &WndAttrs<&str>) -> Self::HWnd {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { HWnd::new(attrs) }
    }

    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &WndAttrs<&str>) {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { window.set_attrs(attrs) }
    }

    fn remove_wnd(&self, window: &Self::HWnd) {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { window.remove() }
    }

    fn update_wnd(&self, window: &Self::HWnd) {
        window.update(self);
    }

    fn new_layer(&self, attrs: &LayerAttrs) -> Self::HLayer {
        HLayer::new(self, attrs)
    }
    fn set_layer_attr(&self, layer: &Self::HLayer, attrs: &LayerAttrs) {
        layer.set_attrs(self, attrs);
    }
    fn remove_layer(&self, layer: &Self::HLayer) {
        layer.remove(self);
    }
}
