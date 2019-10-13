use cgmath::Point2;

use super::{native, HWnd, HWndInner, Wm};
use crate::iface;

/// Wraps `WndListener<Wm>` to create a `WndListener<native::Wm>`.
pub struct NativeWndListener(pub Box<dyn iface::WndListener<Wm>>);

fn from_native_hwnd(hwnd: &native::HWnd) -> HWnd {
    HWnd {
        inner: HWndInner::Native(hwnd.clone()),
    }
}

/// Argument conversion
macro_rules! forward_arg {
    ([hwnd: $x:expr]) => {
        &from_native_hwnd($x)
    };
    ([wm: $x:expr]) => {
        Wm::from_native_wm($x)
    };
    ($x:ident) => {
        $x
    };
}

/// Forward a method call to an inner type by converting arguments using
/// `forward_arg`.
macro_rules! forward {
    ($inner:expr, $method:ident, $($arg:tt),*$(,)*) => {
        $inner.$method($(forward_arg!($arg)),*)
    };
}

impl iface::WndListener<native::Wm> for NativeWndListener {
    fn close_requested(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, close_requested, [wm: wm], [hwnd: hwnd])
    }

    fn resize(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, resize, [wm: wm], [hwnd: hwnd])
    }

    fn dpi_scale_changed(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, dpi_scale_changed, [wm: wm], [hwnd: hwnd])
    }

    fn mouse_motion(&self, wm: native::Wm, hwnd: &native::HWnd, loc: Point2<f32>) {
        forward!(self.0, mouse_motion, [wm: wm], [hwnd: hwnd], loc)
    }

    fn mouse_leave(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, mouse_leave, [wm: wm], [hwnd: hwnd])
    }

    fn mouse_drag(
        &self,
        wm: native::Wm,
        hwnd: &native::HWnd,
        loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn iface::MouseDragListener<native::Wm>> {
        let drag_listener = forward!(self.0, mouse_drag, [wm: wm], [hwnd: hwnd], loc, button);

        Box::new(NativeMouseDragListener(drag_listener))
    }
}

/// Wraps `MouseDragListener<Wm>` to create a `MouseDragListener<native::Wm>`.
struct NativeMouseDragListener(Box<dyn iface::MouseDragListener<Wm>>);

impl iface::MouseDragListener<native::Wm> for NativeMouseDragListener {
    fn mouse_motion(&self, wm: native::Wm, hwnd: &native::HWnd, loc: Point2<f32>) {
        forward!(self.0, mouse_motion, [wm: wm], [hwnd: hwnd], loc)
    }

    fn mouse_down(&self, wm: native::Wm, hwnd: &native::HWnd, loc: Point2<f32>, button: u8) {
        forward!(self.0, mouse_down, [wm: wm], [hwnd: hwnd], loc, button)
    }

    fn mouse_up(&self, wm: native::Wm, hwnd: &native::HWnd, loc: Point2<f32>, button: u8) {
        forward!(self.0, mouse_up, [wm: wm], [hwnd: hwnd], loc, button)
    }

    fn cancel(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, cancel, [wm: wm], [hwnd: hwnd])
    }
}
