#![allow(unused_parens)]
use cgmath::{Point2, Vector2};

use super::{native, AccelTable, HWnd, HWndInner, Wm};
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
    ($other:tt) => {
        $other
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

    fn nc_hit_test(&self, wm: native::Wm, hwnd: &native::HWnd, loc: Point2<f32>) -> iface::NcHit {
        forward!(self.0, nc_hit_test, [wm: wm], [hwnd: hwnd], loc)
    }

    fn interpret_event(
        &self,
        wm: native::Wm,
        hwnd: &native::HWnd,
        ctx: &mut dyn iface::InterpretEventCtx<native::AccelTable>,
    ) {
        let mut ctx = TestingInterpretEventCtx(ctx);
        forward!(self.0, interpret_event, [wm: wm], [hwnd: hwnd], (&mut ctx))
    }

    fn validate_action(
        &self,
        wm: native::Wm,
        hwnd: &native::HWnd,
        action: iface::ActionId,
    ) -> iface::ActionStatus {
        forward!(self.0, validate_action, [wm: wm], [hwnd: hwnd], action)
    }

    fn perform_action(&self, wm: native::Wm, hwnd: &native::HWnd, action: iface::ActionId) {
        forward!(self.0, perform_action, [wm: wm], [hwnd: hwnd], action)
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

    fn scroll_motion(
        &self,
        wm: native::Wm,
        hwnd: &native::HWnd,
        loc: Point2<f32>,
        delta: &iface::ScrollDelta,
    ) {
        forward!(self.0, scroll_motion, [wm: wm], [hwnd: hwnd], loc, delta)
    }

    fn scroll_gesture(
        &self,
        wm: native::Wm,
        hwnd: &native::HWnd,
        loc: Point2<f32>,
    ) -> Box<dyn iface::ScrollListener<native::Wm>> {
        let scroll_listener = forward!(self.0, scroll_gesture, [wm: wm], [hwnd: hwnd], loc);

        Box::new(NativeScrollListener(scroll_listener))
    }
}

/// Wraps `InterpretEventCtx<native::AccelTable>` to create a `InterpretEventCtx<AccelTable>`.
struct TestingInterpretEventCtx<'a>(&'a mut dyn iface::InterpretEventCtx<native::AccelTable>);

impl iface::InterpretEventCtx<AccelTable> for TestingInterpretEventCtx<'_> {
    fn use_accel(&mut self, haccel: &AccelTable) {
        self.0.use_accel(&haccel.native);
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

/// Wraps `ScrollListener<Wm>` to create a `ScrollListener<native::Wm>`.
struct NativeScrollListener(Box<dyn iface::ScrollListener<Wm>>);

impl iface::ScrollListener<native::Wm> for NativeScrollListener {
    fn motion(
        &self,
        wm: native::Wm,
        hwnd: &native::HWnd,
        delta: &iface::ScrollDelta,
        velocity: Vector2<f32>,
    ) {
        forward!(self.0, motion, [wm: wm], [hwnd: hwnd], delta, velocity)
    }

    fn start_momentum_phase(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, start_momentum_phase, [wm: wm], [hwnd: hwnd])
    }

    fn end(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, end, [wm: wm], [hwnd: hwnd])
    }

    fn cancel(&self, wm: native::Wm, hwnd: &native::HWnd) {
        forward!(self.0, cancel, [wm: wm], [hwnd: hwnd])
    }
}
