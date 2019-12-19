//! The GTK backend.
use super::iface;
use std::{marker::PhantomData, ops::Range, time::Duration};

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

// Borrow some modules from `unix` backend
#[path = "unix/bitmap.rs"]
mod bitmap;
#[path = "unix/text.rs"]
mod text;
pub use self::{
    bitmap::{Bitmap, BitmapBuilder},
    text::{CharStyle, TextLayout},
};

#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
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
        unimplemented!()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        unimplemented!()
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        unimplemented!()
    }

    fn invoke_after(self, delay: Range<Duration>, f: impl FnOnce(Self) + 'static) -> Self::HInvoke {
        unimplemented!()
    }

    fn cancel_invoke(self, hinv: &Self::HInvoke) {
        unimplemented!()
    }

    fn enter_main_loop(self) -> ! {
        unimplemented!()
    }

    fn terminate(self) {
        unimplemented!()
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HLayer;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke;
