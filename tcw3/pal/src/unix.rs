//! The backend for a *nix system.
//!
//! This backend is backed by the following software components:
//!
//!  - winit (usually backed by X11 or Wayland) for window management
//!  - Vulkan (WIP) or a software renderer for composition,
//!  - Cairo for 2D drawing (WIP)
//!  - FreeType/Pango/fontconfig for text rendering (WIP).
//!
use std::marker::PhantomData;

use super::{
    iface,
    prelude::MtLazyStatic,
    winit::{HWndCore, WinitEnv, WinitWm, WinitWmCore},
};

// Define a global instance of `WinitEnv`.
//
// This is a part of boilerplate code of `super::winit` that exists because we
// delegate the window management to `super::winit`.
static WINIT_ENV: WinitEnv<Wm, WndContent> = WinitEnv::new();

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

pub type HWnd = HWndCore;

mod bitmap;
mod comp;
mod text;
pub use self::{
    bitmap::{Bitmap, BitmapBuilder},
    comp::{HLayer, WndContent},
    text::{CharStyle, TextLayout},
};

/// Provides an access to the window system.
///
/// `Wm` is only accessible by the application's main thread. Therefore, the
/// ownership of `Wm` can be used as an evidence that the main thread has the
/// control.
#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

mt_lazy_static! {
    static <Wm> ref COMP: comp::Compositor => |wm| comp::Compositor::new(wm);
}

impl Wm {
    /// Get the global `WinitWmCore` instance.
    ///
    /// Use `WinitWmCore::wm` for the conversion in the other way around.
    fn winit_wm_core(self) -> &'static WinitWmCore<Wm, WndContent> {
        WINIT_ENV.wm_with_wm(self)
    }

    /// Get the global `Compositor` instance.
    fn comp(self) -> &'static comp::Compositor {
        COMP.get_with_wm(self)
    }
}

// `super::winit` uses this `impl` for the framework's operation
impl WinitWm for Wm {
    fn hwnd_core_to_hwnd(self, hwnd: &HWndCore) -> Self::HWnd {
        hwnd.clone()
    }

    fn init(self) {
        // Force the initialization of `COMP`. We should this now because if
        // we do it later, we might not be able to access winit's `EventLoop`,
        // which we need to initialize `Compositor`.
        //
        // Astoundingly un-Rusty... TODO: Perhaps make this more Rusty?
        // I think we could add a new type parameter to `WinitEnv` or a new
        // associate type to `WinitWm` to allow storing custom data in
        // `WinitWmCore`. Note that we can't store it in `Wm` because `Wm` is
        // just a marker type indicating the main thread. But, do not forget
        // to think about the practical benefits! (Do not blindly follow the
        // "best practices".)
        let _ = COMP.get_with_wm(self);
    }
}

impl iface::Wm for Wm {
    type HWnd = HWnd;
    type HLayer = HLayer;
    type Bitmap = Bitmap;

    unsafe fn global_unchecked() -> Wm {
        Wm {
            _no_send_sync: PhantomData,
        }
    }

    fn is_main_thread() -> bool {
        WINIT_ENV.is_main_thread()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        WINIT_ENV.invoke_on_main_thread(move |winit_wm| f(winit_wm.wm()));
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        self.winit_wm_core()
            .invoke(move |winit_wm| f(winit_wm.wm()));
    }

    fn enter_main_loop(self) -> ! {
        WINIT_ENV.wm_with_wm(self).enter_main_loop();
    }

    fn terminate(self) {
        WINIT_ENV.wm_with_wm(self).terminate();
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        self.winit_wm_core().new_wnd(attrs, |winit_wnd, layer| {
            self.comp().new_wnd(winit_wnd, layer)
        })
    }

    fn set_wnd_attr(self, hwnd: &Self::HWnd, attrs: WndAttrs<'_>) {
        self.winit_wm_core().set_wnd_attr(hwnd, attrs)
    }

    fn remove_wnd(self, hwnd: &Self::HWnd) {
        self.winit_wm_core().remove_wnd(hwnd)
    }

    fn update_wnd(self, hwnd: &Self::HWnd) {
        self.winit_wm_core().update_wnd(hwnd)
    }

    fn get_wnd_size(self, hwnd: &Self::HWnd) -> [u32; 2] {
        self.winit_wm_core().get_wnd_size(hwnd)
    }

    fn get_wnd_dpi_scale(self, hwnd: &Self::HWnd) -> f32 {
        self.winit_wm_core().get_wnd_dpi_scale(hwnd)
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        self.comp().new_layer(attrs)
    }
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs) {
        self.comp().set_layer_attr(layer, attrs)
    }
    fn remove_layer(self, layer: &Self::HLayer) {
        self.comp().remove_layer(layer)
    }
}
