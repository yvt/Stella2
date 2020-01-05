//! The Windows backend.
use super::iface;
use std::{cell::Cell, marker::PhantomData, ops::Range, time::Duration};

mod bitmap;
mod eventloop;

pub use self::{
    bitmap::{Bitmap, BitmapBuilder, CharStyle, TextLayout},
    eventloop::HInvoke,
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
        unimplemented!()
    }

    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_>) {
        unimplemented!()
    }

    fn remove_wnd(self, window: &Self::HWnd) {
        unimplemented!()
    }

    fn update_wnd(self, window: &Self::HWnd) {
        unimplemented!()
    }

    fn get_wnd_size(self, window: &Self::HWnd) -> [u32; 2] {
        unimplemented!()
    }

    fn get_wnd_dpi_scale(self, window: &Self::HWnd) -> f32 {
        unimplemented!()
    }

    fn request_update_ready_wnd(self, window: &Self::HWnd) {
        unimplemented!()
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        unimplemented!()
    }
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs) {
        unimplemented!()
    }
    fn remove_layer(self, layer: &Self::HLayer) {
        unimplemented!()
    }
}

struct AssertSend<T>(T);
unsafe impl<T> Send for AssertSend<T> {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HLayer;
