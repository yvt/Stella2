//! The backend for macOS, Cocoa, and Core Graphics.
use cfg_if::cfg_if;
use std::{marker::PhantomData, ops::Range, time::Duration};

use super::iface;

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type MtSticky<T> = super::MtSticky<T, Wm>;

mod bitmap;
mod drawutils;
mod layer;
mod text;
mod utils;
pub use self::bitmap::{Bitmap, BitmapBuilder};
pub use self::layer::HLayer;
pub use self::text::{CharStyle, TextLayout};

use cocoa::{
    appkit,
    appkit::{NSApplication, NSApplicationActivationPolicy},
    base::nil,
};
use objc::{msg_send, sel, sel_impl};

mod window;
mod timer;
pub use self::{window::HWnd, timer::HInvoke};

use self::utils::{is_main_thread, IdRef};

/// Provides an access to the window system.
///
/// `Wm` is only accessible by the application's main thread. Therefore, the
/// ownership of `Wm` can be used as an evidence that the main thread has the
/// control.
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
        is_main_thread()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        dispatch::Queue::main().r#async(|| f(unsafe { Self::global_unchecked() }));
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        // Give `Send` uncondionally because we don't `Send` actually
        // (we are already on the main thread)
        struct AssertSend<T>(T);
        unsafe impl<T> Send for AssertSend<T> {}
        let cell = AssertSend(f);

        Self::invoke_on_main_thread(move |wm| {
            let AssertSend(f) = cell;
            f(wm);
        });
    }

    fn invoke_after(self, delay: Range<Duration>, f: impl FnOnce(Self) + 'static) -> Self::HInvoke {
        timer::invoke_after(self, delay, f)
    }

    fn cancel_invoke(self, hinv: &Self::HInvoke) {
        timer::cancel_invoke(self, hinv)
    }

    fn enter_main_loop(self) -> ! {
        unsafe {
            let app = appkit::NSApp();
            app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
            );
            app.finishLaunching();
            app.run();
        }

        std::process::exit(0);
    }

    fn terminate(self) {
        unsafe {
            let app = appkit::NSApp();
            let () = msg_send![app, terminate: nil];
        }
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        HWnd::new(self, attrs)
    }

    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_>) {
        window.set_attrs(self, attrs);
    }

    fn remove_wnd(self, window: &Self::HWnd) {
        window.remove(self);
    }

    fn update_wnd(self, window: &Self::HWnd) {
        window.update(self);
    }

    fn request_update_ready_wnd(self, window: &Self::HWnd) {
        window.request_update_ready(self);
    }

    fn get_wnd_size(self, window: &Self::HWnd) -> [u32; 2] {
        window.get_size(self)
    }

    fn get_wnd_dpi_scale(self, window: &Self::HWnd) -> f32 {
        window.get_dpi_scale(self)
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        HLayer::new(self, attrs)
    }
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs) {
        layer.set_attrs(self, attrs);
    }
    fn remove_layer(self, layer: &Self::HLayer) {
        layer.remove(self);
    }
}
