//! The backend for macOS, Cocoa, and Core Graphics.
use cfg_if::cfg_if;
use cocoa::{
    appkit,
    appkit::{NSApplication, NSApplicationActivationPolicy},
    base::nil,
};
use objc::{msg_send, sel, sel_impl};
use std::marker::PhantomData;

use super::{iface, LayerAttrs, WndAttrs};

mod bitmap;
mod drawutils;
mod layer;
mod text;
mod utils;
pub use self::bitmap::{Bitmap, BitmapBuilder};
pub use self::layer::HLayer;
pub use self::text::{CharStyle, TextLayout};

cfg_if! {
    if #[cfg(feature = "macos_winit")] {
        mod winitwindow;
        pub use self::winitwindow::HWnd;

        use super::winit::WinitEnv;
        static WINIT_ENV: WinitEnv<WM, winitwindow::WndContent> = WinitEnv::new();
    } else {
        mod window;
        pub use self::window::HWnd;

        use self::utils::{is_main_thread, IdRef};
    }
}

/// Provides an access to the window system.
///
/// `WM` is only accessible by the application's main thread. Therefore, the
/// ownership of `WM` can be used as an evidence that the main thread has the
/// control.
#[derive(Debug, Clone, Copy)]
pub struct WM {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

impl iface::WM for WM {
    type HWnd = HWnd;
    type HLayer = HLayer;
    type Bitmap = Bitmap;

    unsafe fn global_unchecked() -> WM {
        WM {
            _no_send_sync: PhantomData,
        }
    }

    #[cfg(not(feature = "macos_winit"))]
    fn is_main_thread() -> bool {
        is_main_thread()
    }

    #[cfg(not(feature = "macos_winit"))]
    fn invoke_on_main_thread(f: impl FnOnce(WM) + Send + 'static) {
        dispatch::Queue::main().r#async(|| f(unsafe { Self::global_unchecked() }));
    }

    #[cfg(not(feature = "macos_winit"))]
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

    #[cfg(not(feature = "macos_winit"))]
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

    #[cfg(not(feature = "macos_winit"))]
    fn terminate(self) {
        unsafe {
            let app = appkit::NSApp();
            let () = msg_send![app, terminate: nil];
        }
    }

    #[cfg(feature = "macos_winit")]
    fn is_main_thread() -> bool {
        WINIT_ENV.is_main_thread()
    }

    #[cfg(feature = "macos_winit")]
    fn invoke_on_main_thread(f: impl FnOnce(WM) + Send + 'static) {
        WINIT_ENV.invoke_on_main_thread(move |winit_wm| f(winit_wm.wm()));
    }

    #[cfg(feature = "macos_winit")]
    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        WINIT_ENV
            .wm_with_wm(self)
            .invoke(move |winit_wm| f(winit_wm.wm()));
    }

    #[cfg(feature = "macos_winit")]
    fn enter_main_loop(self) -> ! {
        WINIT_ENV.wm_with_wm(self).enter_main_loop();
    }

    #[cfg(feature = "macos_winit")]
    fn terminate(self) {
        WINIT_ENV.wm_with_wm(self).terminate();
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { HWnd::new(attrs) }
    }

    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_>) {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { window.set_attrs(attrs) }
    }

    fn remove_wnd(self, window: &Self::HWnd) {
        // Having a reference to `WM` means we are on a main thread, so
        // this is safe
        unsafe { window.remove() }
    }

    fn update_wnd(self, window: &Self::HWnd) {
        window.update(self);
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
