//! The Windows backend.
use super::iface;
use std::{cell::Cell, marker::PhantomData, ops::Range, time::Duration};

mod bitmap;
mod codecvt;
mod comp;
mod eventloop;
mod window;

pub use self::{
    bitmap::{Bitmap, BitmapBuilder, CharStyle, TextLayout},
    eventloop::HInvoke,
    window::HWnd,
};

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

thread_local! {
    static IS_MAIN_THREAD: Cell<bool> = Cell::new(false);
}

impl iface::Wm for Wm {
    type HWnd = HWnd;
    type HLayer = HLayer;
    type HInvoke = HInvoke;
    type Bitmap = Bitmap;

    unsafe fn global_unchecked() -> Wm {
        Wm {
            _no_send_sync: PhantomData,
        }
    }

    fn is_main_thread() -> bool {
        eventloop::is_main_thread()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        eventloop::invoke_on_main_thread(Box::new(move |wm| f(wm)));
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        // This is safe because we know we are already in the main thread
        let f = AssertSend(f);
        eventloop::invoke(self, Box::new(move |wm| (f.0)(wm)));
    }

    fn invoke_after(self, delay: Range<Duration>, f: impl FnOnce(Self) + 'static) -> Self::HInvoke {
        eventloop::invoke_after(self, delay, Box::new(f))
    }

    fn cancel_invoke(self, hinv: &Self::HInvoke) {
        eventloop::cancel_invoke(self, hinv);
    }

    fn enter_main_loop(self) -> ! {
        eventloop::enter_main_loop();
        std::process::exit(0);
    }

    fn terminate(self) {
        eventloop::terminate();
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        window::new_wnd(self, attrs)
    }

    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_>) {
        window::set_wnd_attr(self, window, attrs)
    }

    fn remove_wnd(self, window: &Self::HWnd) {
        window::remove_wnd(self, window)
    }

    fn update_wnd(self, window: &Self::HWnd) {
        window::update_wnd(self, window)
    }

    fn get_wnd_size(self, window: &Self::HWnd) -> [u32; 2] {
        window::get_wnd_size(self, window)
    }

    fn get_wnd_dpi_scale(self, window: &Self::HWnd) -> f32 {
        window::get_wnd_dpi_scale(self, window)
    }

    fn request_update_ready_wnd(self, window: &Self::HWnd) {
        window::request_update_ready_wnd(self, window)
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        log::warn!("new_layer: stub!");
        HLayer
    }
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs) {
        log::warn!("set_layer_attr: stub!");
    }
    fn remove_layer(self, layer: &Self::HLayer) {
        log::warn!("remove_layer: stub!");
    }
}

struct AssertSend<T>(T);
unsafe impl<T> Send for AssertSend<T> {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HLayer;
