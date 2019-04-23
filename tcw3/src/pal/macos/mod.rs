use super::{traits, types};

pub struct WM {}

impl WM {
    pub fn global() -> &'static WM {
        // TODO: check main thread
        &WM {}
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
