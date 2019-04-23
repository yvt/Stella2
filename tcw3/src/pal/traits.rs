use super::types::WndAttrs;

pub trait WM {
    /// A window handle type.
    type HWnd: Clone;

    fn enter_main_loop(&self);

    fn new_wnd(&self, attrs: &WndAttrs<&str>) -> &Self::HWnd;
    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &WndAttrs<&str>);
    fn remove_wnd(&self, window: &Self::HWnd);
}
