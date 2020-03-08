//! The GTK backend.
use super::iface;
use std::{cell::RefCell, marker::PhantomData, mem::MaybeUninit, ops::Range, time::Duration};

use crate::MtLock;

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

mod comp;
mod textinput;
mod timer;
mod window;
pub use self::{comp::HLayer, textinput::HTextInputCtx, timer::HInvoke, window::HWnd};

#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

static TIMER_POOL: MtLock<RefCell<timer::TimerPool>, Wm> =
    MtLock::new(RefCell::new(timer::TimerPool::new()));

impl iface::Wm for Wm {
    type HWnd = HWnd;
    type HLayer = HLayer;
    type HInvoke = HInvoke;
    type HTextInputCtx = HTextInputCtx;
    type Bitmap = Bitmap;

    unsafe fn global_unchecked() -> Wm {
        Wm {
            _no_send_sync: PhantomData,
        }
    }

    fn is_main_thread() -> bool {
        if !gtk::is_initialized() {
            return is_main_thread_inner();
        }

        #[cold]
        fn is_main_thread_inner() -> bool {
            // Panic here if GTK fails to initialize
            gtk::init().unwrap();
            true
        }

        gtk::is_initialized_main_thread()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        // TODO: see if this works when `!gtk::is_initialized()`

        let f = MaybeUninit::new(f);

        glib::source::idle_add(move || {
            // We assume this closure will never dropped without being called.
            // Even if it should happen, `f` just gets leaked.
            unsafe {
                // This is safe because we know we are already in the main thread
                let wm = Self::global_unchecked();

                // This closure is called only once because it returns
                // `Continue(false)`. So, this is safe.
                f.as_ptr().read()(wm);
            }
            glib::source::Continue(false)
        });
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        // This is safe because we know we are already in the main thread
        let f = AssertSend(f);
        Self::invoke_on_main_thread(move |wm| (f.0)(wm));
    }

    fn invoke_after(self, delay: Range<Duration>, f: impl FnOnce(Self) + 'static) -> Self::HInvoke {
        // This is safe because we know we are already in the main thread
        let mut f = Some(AssertSend(f));

        let mut pool = TIMER_POOL.get_with_wm(self).borrow_mut();
        pool.insert(move |hinvoke| {
            let interval = delay.start.as_millis() as u32;
            // TODO: Use `timeout_add`.
            glib::source::timeout_add_local(interval, move || {
                // This closure may be dropped early if the invocation was
                // cancelled, hence the use of `Some` instead of `MaybeUninit`.

                // This is safe because we know we are already in the main thread
                let wm = unsafe { Self::global_unchecked() };

                // Remove `SourceId` from `TIMER_POOL` so that we don't remove
                // a wrong source with the same re-used `SourceId`
                // in`cancel_invoke`
                TIMER_POOL.get_with_wm(wm).borrow_mut().remove(&hinvoke);

                // This closure is called only once because it returns
                // `Continue(false)`. So, this is safe.
                let f = unsafe {
                    f.take()
                        .unwrap_or_else(|| std::hint::unreachable_unchecked())
                };
                (f.0)(wm);

                glib::source::Continue(false)
            })
        })
    }

    fn cancel_invoke(self, hinv: &Self::HInvoke) {
        if let Some(source_id) = TIMER_POOL.get_with_wm(self).borrow_mut().remove(hinv) {
            glib::source::source_remove(source_id);
        }
    }

    fn enter_main_loop(self) -> ! {
        // This is safe because the posession of `Wm` means GTK is already
        // initialized and we are currently in the main thread.
        unsafe {
            gtk_sys::gtk_main();
        }

        std::process::exit(0);
    }

    fn terminate(self) {
        debug_assert!(Self::is_main_thread());
        // This is safe because the posession of `Wm` means GTK is already
        // initialized and we are currently in the main thread.
        unsafe {
            gtk_sys::gtk_main_quit();
        }
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        HWnd::new_wnd(self, attrs)
    }

    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_>) {
        window.set_wnd_attr(self, attrs)
    }

    fn remove_wnd(self, window: &Self::HWnd) {
        window.remove_wnd(self)
    }

    fn update_wnd(self, window: &Self::HWnd) {
        window.update_wnd(self)
    }

    fn get_wnd_size(self, window: &Self::HWnd) -> [u32; 2] {
        window.get_wnd_size(self)
    }

    fn get_wnd_dpi_scale(self, window: &Self::HWnd) -> f32 {
        window.get_wnd_dpi_scale(self)
    }

    fn is_wnd_focused(self, window: &Self::HWnd) -> bool {
        window.is_wnd_focused(self)
    }

    fn request_update_ready_wnd(self, window: &Self::HWnd) {
        window.request_update_ready_wnd(self)
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        window::COMPOSITOR
            .get_with_wm(self)
            .borrow_mut()
            .new_layer(attrs)
    }
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs) {
        window::COMPOSITOR
            .get_with_wm(self)
            .borrow_mut()
            .set_layer_attr(layer, attrs)
    }
    fn remove_layer(self, layer: &Self::HLayer) {
        window::COMPOSITOR
            .get_with_wm(self)
            .borrow_mut()
            .remove_layer(layer)
    }

    fn new_text_input_ctx(
        self,
        _hwnd: &Self::HWnd,
        _listener: Box<dyn iface::TextInputCtxListener<Self>>,
    ) -> Self::HTextInputCtx {
        HTextInputCtx {}
    }

    fn text_input_ctx_set_active(self, _: &Self::HTextInputCtx, _active: bool) {
        // TODO
    }

    fn remove_text_input_ctx(self, _: &Self::HTextInputCtx) {
        // TODO
    }
}

struct AssertSend<T>(T);
unsafe impl<T> Send for AssertSend<T> {}
