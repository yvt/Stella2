use super::types::WndAttrs;

pub trait WM {
    /// A window handle type.
    type HWnd: Clone;

    fn enter_main_loop(&self);
    fn terminate(&self);

    fn new_wnd(&self, attrs: &WndAttrs<Self::HWnd, &str>) -> Self::HWnd;
    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &WndAttrs<Self::HWnd, &str>);
    fn remove_wnd(&self, window: &Self::HWnd);
}

/// Window event handlers.
///
/// The receiver is immutable because event handlers may manipulate windows,
/// which in turn might cause other event handlers to be called.
pub trait WndListener<HWnd> {
    /// The user has attempted to close a window. Returns `true` if the window
    /// can be closed.
    fn close_requested(&self, _: &HWnd) -> bool {
        true
    }

    /// A window has been closed.
    fn close(&self, _: &HWnd) {}

    // TODO: more events
}
