use cocoa::appkit;
use fragile::Fragile;
use lazy_static::lazy_static;

use super::{traits, types};

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
                Fragile::new(WM {})
            };
        }

        GLOBAL_WM.get()
    }
}

impl traits::WM for WM {
    type HWnd = ();

    fn enter_main_loop(&self) {
        unimplemented!()
    }

    fn new_wnd(&self, attrs: &types::WndAttrs<&str>) -> &Self::HWnd {
        unimplemented!()
    }

    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &types::WndAttrs<&str>) {
        unimplemented!()
    }

    fn remove_wnd(&self, window: &Self::HWnd) {
        unimplemented!()
    }
}
