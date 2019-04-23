use cocoa::{
    appkit,
    appkit::{NSApplication, NSApplicationActivationPolicy},
};
use fragile::Fragile;
use lazy_static::lazy_static;
use objc::{msg_send, sel, sel_impl};

use super::{traits, types};

mod utils;
mod window;
use self::utils::{with_autorelease_pool, IdRef};
pub use self::window::HWnd;

pub struct WM {}

impl WM {
    pub fn global() -> &'static WM {
        lazy_static! {
            static ref GLOBAL_WM: Fragile<WM> = {
                // Mark the current thread as the main thread
                unsafe {
                    appkit::NSApp();
                }

                // `Fragile` wraps `!Send` types and performs run-time
                // main thread checking
                Fragile::new(WM::new())
            };
        }

        GLOBAL_WM.get()
    }

    fn new() -> Self {
        Self {}
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

    fn new_wnd(&self, attrs: &types::WndAttrs<&str>) -> Self::HWnd {
        HWnd::new(attrs)
    }

    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &types::WndAttrs<&str>) {
        window.set_attrs(attrs)
    }

    fn remove_wnd(&self, window: &Self::HWnd) {
        window.remove();
    }
}
