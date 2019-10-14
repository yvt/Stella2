//! The testing backend.
//!
//! This backend wraps the default native backend defined by a module alias
//! named `crate::native`. By default, the backend simply passes all requests
//! through and behaves no different from the underlying native backend. The
//! testing backend *must be activated at runtime* in order to use its testing
//! functionality. This way, normal applications aren't affected by the
//! `testing` feature flag, though you in general should try to disable it in
//! release builds.
//!
//! When activated, the testing backend establishes its own "main thread" and
//! redirects all methods to itself. In this state, the native backend isn't
//! used at all. Window contents are rendered into off-screen images, which can
//! be examined and read out using an API specific to the testing backend. Test
//! drivers can send artificial input events to virtual windows.
//!
//! # Platform independency
//!
//! The testing backend does its best to achieve platform independency by using
//! the same external libraries for all platforms. However, since Pango uses a
//! platform-specific backend, such as fontconfig that uses system/user-specific
//! configuration files, font rendering might be different from one environment
//! to another.
//!
//! In the future, we could switch to a more platform-independent solution such
//! as [`rusttype`], but it also should be kept in mind that such a solution
//! will add development/maintenance effort and reduce test coverage.
//!
//! [`rusttype`]: https://github.com/redox-os/rusttype
//!
//! # Activation
//!
//! The testing backend is activated when [`with_testing_wm`] is called. The
//! activation will fail if the native backend has already been selected at this
//! point. The native backend is automatically selected when a method of `Wm`
//! (usually `Wm::global()`) is called for the first time. Therefore, test
//! programs must call `with_testing_wm` as soon as possible and must not use
//! `Wm::global()`. Once a backend is chosen, `Wm::global()` points to the
//! testing backend. Normal applications just directly use `Wm` and they get the
//! native backend as in the case if the `testing` feature flag isn't used.
//!
//! [`with_testing_wm`]: crate::testing::with_testing_wm
//!
//! The choice of a backend remains permanent throughout a program's lifetime.
//! Once the native backend is initialized, the testing backend cannot be
//! activated anymore because `Wm` is already associated to the native backend's
//! main thread, i.e., there may be an instance of `Wm` which is owned by the
//! thread. Creating an instance of the same `Wm` for a different thread
//! violates `Wm`'s safety requirements and compromises thread safety.
//!
//! This could have been avoided by having a separate `Wm` for each backend.
//! However, I decided not to go down this way because:
//!
//!  - Other components including `tcw3` assumes that there is one and only one
//!    `Wm` type that represents the default backend for a target platform. Many
//!    types should be made generic to support swapping `Wm` types.
//!  - But generic types slows down the compilation and iterative development of
//!    an application because code generation has to wait until concrete types
//!    are determined and monomorphization takes place.
//!  - Global variables created by `mt_lazy_static` cannot be generic. Actually,
//!    this can be circumvented by having `HashMap<TypeId, T>`, but this comes
//!    with a runtime cost, code size cost, and extra code complexity.
//!
//! # Writing tests
//!
//! Testing code should use [`run_test`] to use the testing backend. A passed
//! closure receives `&dyn `[`TestingWm`], through which methods specific to
//! the backend can be called. Testing code will compile fine whether the
//! feature flag is set or not as long as it does not access other APIs which
//! are only available if the backend is enabled.
//!
//!     use tcw3_pal::{testing, prelude::*};
//!     use std::time::{Instant, Duration};
//!
//!     #[test]
//!     fn create_wnd() {
//!         testing::run_test(|twm| {
//!             // This block might or might not run depending on whether
//!             // the feature flag is set
//!             let wm = twm.wm();
//!             let wnd = wm.new_wnd(Default::default());
//!
//!             twm.step_until(Instant::now() + Duration::from_millis(100));
//!         });
//!     }
//!
//! [`run_test`]: crate::testing::run_test
//! [`TestingWm`]: crate::testing::TestingWm
//!
//! If it uses other PAL objects such as `Bitmap` and doesn't use `Wm`, the
//! testing code should still use [`run_test`] because they are also subject to
//! the backend selection.
use atom2::SetOnceAtom;
use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use std::{
    marker::PhantomData,
    panic,
    sync::mpsc::sync_channel,
    thread::{self, ThreadId},
};

use super::{iface, iface::Wm as _, native, prelude::MtLazyStatic};

mod eventloop;
mod screen;
mod uniqpool;
mod wmapi;
mod wndlistenershim;
pub use self::wmapi::TestingWm;

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

// Borrow some modules from `unix` backend
#[path = "unix/bitmap.rs"]
mod bitmap;
#[path = "unix/text.rs"]
mod text;

/// Activate the testing backend and call the given function on the main thread.
///
/// Panics if the native backend has already been initialized.
/// See [the module documentation](index.html) for more.
pub fn with_testing_wm<R: Send + 'static>(
    cb: impl FnOnce(Wm) -> R + Send + panic::UnwindSafe + 'static,
) -> R {
    boot_testing_backend();

    if let Some(&Backend::Native) = Wm::backend_or_none() {
        panic!("Cannot start the testing backend; the native backend is already active.");
    }

    let (send, recv) = sync_channel(1);
    Wm::invoke_on_main_thread(move |wm| {
        let result = panic::catch_unwind(|| cb(wm));

        send.send(result).unwrap();
    });

    match recv.recv().unwrap() {
        Ok(x) => x,
        Err(x) => panic::resume_unwind(x),
    }
}

/// Call `with_testing_wm` if the testing backend is enabled. Otherwise,
/// output a warning message and return without calling the givne function.
///
/// This function is available even if the `testing` feature flag is disabled.
pub fn run_test(cb: impl FnOnce(&dyn TestingWm) + Send + panic::UnwindSafe + 'static) {
    with_testing_wm(|wm| cb(&wm));
}

// TODO: Add artificial inputs and outputs

#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

// ============================================================================
//
// Backend choice and main thread

enum Backend {
    Native,
    Testing {
        main_thread: ThreadId,
        sender: eventloop::DispatchSender,
    },
}

enum BackendAndWm {
    Native {
        wm: native::Wm,
    },
    Testing {
        wm: Wm,
        sender: &'static eventloop::DispatchSender,
    },
}

/// The currently chosen backend. This can be set only once throughout
/// a program's lifetime.
static BACKEND_CHOICE: SetOnceAtom<Box<Backend>> = SetOnceAtom::empty();

impl Wm {
    /// Get the current choice of a backend. If none are chosen, the native
    /// backend will be initialized.
    fn backend() -> &'static Backend {
        if BACKEND_CHOICE.get().is_none() {
            // Try setting the native backend. This might fail.
            let _ = BACKEND_CHOICE.store(Some(Box::new(Backend::Native)));
        }
        &**BACKEND_CHOICE.get().unwrap()
    }

    fn backend_or_none() -> Option<&'static Backend> {
        BACKEND_CHOICE.get().map(|x| &**x)
    }

    fn backend_and_wm(self) -> BackendAndWm {
        match Self::backend() {
            // If we have `Wm`, its usage is congruent with `native::Wm`,
            // so this is safe
            Backend::Native => BackendAndWm::Native {
                wm: unsafe { native::Wm::global_unchecked() },
            },
            Backend::Testing { sender, .. } => BackendAndWm::Testing { wm: self, sender },
        }
    }

    /// Convert `native::Wm` to our `Wm`. This is an inverse of `backend_and_wm`,
    /// assuming the current backend is `Backend::Native`.
    fn from_native_wm(native_wm: native::Wm) -> Self {
        // `Wm::backend()` is okay actually, but generates extra code. The cases
        // handled only by `Wm::backend()` are pathological and artificial, I
        // don't know how they can be produced
        if let Some(&Backend::Native) = Wm::backend_or_none() {
            unsafe { Self::global_unchecked() }
        } else {
            panic!("`testing` is not configured (currently or anymore) to use the native backend");
        }
    }
}

impl BackendAndWm {
    fn native_wm(&self) -> Option<native::Wm> {
        match self {
            &BackendAndWm::Native { wm } => Some(wm),
            &BackendAndWm::Testing { .. } => None,
        }
    }
}

/// Initialize the testing backend. Does nothing if some backend has already
/// been chosen and initialized.
fn boot_testing_backend() {
    if BACKEND_CHOICE.get().is_none() {
        try_start_testing_main_thread();
    }
}

/// Try initializing the testing backend. Does nothing if some backend has
/// already been chosen and initialized. `BACKEND_CHOICE` is guaranteed to
/// contain some value when the function returns.
#[cold]
fn try_start_testing_main_thread() {
    let (ready_send, ready_recv) = sync_channel(1);

    thread::spawn(move || {
        // Try setting the backend. This might fail if there is already one set.
        let (send, recv) = eventloop::dispatch_channel();
        let backend = Backend::Testing {
            main_thread: thread::current().id(),
            sender: send,
        };

        let fail = BACKEND_CHOICE.store(Some(Box::new(backend))).is_err();
        ready_send.send(()).unwrap();
        if fail {
            return;
        }

        // If successful, that means we are on the main thread.
        let wm = Wm::global();
        wm.set_dispatch_receiver(recv);

        wm.enter_main_loop();
    });

    // Proceed when `BACKEND_CHOICE` is finalized
    let () = ready_recv.recv().unwrap();
}

// ============================================================================

mt_lazy_static! {
    static <Wm> ref SCREEN: screen::Screen => screen::Screen::new;
}

impl wmapi::TestingWm for Wm {
    fn wm(&self) -> crate::Wm {
        *self
    }

    fn step(&self) {
        self.step_timeout(None);
    }

    fn step_until(&self, till: std::time::Instant) {
        let duration = till.saturating_duration_since(std::time::Instant::now());
        self.step_timeout(Some(duration));
    }

    fn hwnds(&self) -> Vec<HWnd> {
        SCREEN
            .get_with_wm(*self)
            .hwnds()
            .into_iter()
            .map(|hwnd| HWnd {
                inner: HWndInner::Testing(hwnd),
            })
            .collect()
    }

    fn wnd_attrs(&self, hwnd: &HWnd) -> Option<wmapi::WndAttrs> {
        let hwnd = match &hwnd.inner {
            HWndInner::Testing(hwnd) => hwnd,
            HWndInner::Native(_) => unreachable!(),
        };
        SCREEN.get_with_wm(*self).wnd_attrs(hwnd)
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
        match Self::backend() {
            Backend::Native => native::Wm::is_main_thread(),
            Backend::Testing { main_thread, .. } => thread::current().id() == *main_thread,
        }
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        match Self::backend() {
            Backend::Native => native::Wm::invoke_on_main_thread(move |native_wm| {
                f(Self::from_native_wm(native_wm));
            }),
            Backend::Testing { sender, .. } => {
                sender.invoke_on_main_thread(f);
            }
        }
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
        match self.backend_and_wm() {
            BackendAndWm::Native { wm } => wm.invoke(move |native_wm| {
                f(Self::from_native_wm(native_wm));
            }),
            BackendAndWm::Testing { .. } => {
                self.invoke_unsend(f);
            }
        }
    }

    fn enter_main_loop(self) -> ! {
        match self.backend_and_wm() {
            BackendAndWm::Native { wm } => wm.enter_main_loop(),
            BackendAndWm::Testing { .. } => {
                // This is not very useful during testing because
                // it blocks the current thread indefinitely.
                panic!("this operation is not allowed for the testing backend");
            }
        }
    }

    fn terminate(self) {
        match self.backend_and_wm() {
            BackendAndWm::Native { wm } => wm.terminate(),
            BackendAndWm::Testing { .. } => {
                panic!("this operation is not allowed for the testing backend");
            }
        }
    }

    fn new_wnd(self, attrs: WndAttrs<'_>) -> Self::HWnd {
        match self.backend_and_wm() {
            BackendAndWm::Native { wm } => {
                let attrs = wnd_attrs_to_native(attrs);
                HWnd {
                    inner: HWndInner::Native(wm.new_wnd(attrs)),
                }
            }
            BackendAndWm::Testing { .. } => {
                let attrs = wnd_attrs_to_testing(attrs);
                let screen = SCREEN.get_with_wm(self);
                HWnd {
                    inner: HWndInner::Testing(screen.new_wnd(attrs)),
                }
            }
        }
    }

    fn set_wnd_attr(self, hwnd: &Self::HWnd, attrs: WndAttrs<'_>) {
        match (self.backend_and_wm(), &hwnd.inner) {
            (BackendAndWm::Native { wm }, HWndInner::Native(hwnd)) => {
                let attrs = wnd_attrs_to_native(attrs);
                wm.set_wnd_attr(hwnd, attrs);
            }
            (BackendAndWm::Testing { .. }, HWndInner::Testing(hwnd)) => {
                let attrs = wnd_attrs_to_testing(attrs);
                SCREEN.get_with_wm(self).set_wnd_attr(hwnd, attrs);
            }
            _ => unreachable!(),
        }
    }

    fn remove_wnd(self, hwnd: &Self::HWnd) {
        match (self.backend_and_wm(), &hwnd.inner) {
            (BackendAndWm::Native { wm }, HWndInner::Native(hwnd)) => {
                wm.remove_wnd(hwnd);
            }
            (BackendAndWm::Testing { .. }, HWndInner::Testing(hwnd)) => {
                SCREEN.get_with_wm(self).remove_wnd(hwnd);
            }
            _ => unreachable!(),
        }
    }

    fn update_wnd(self, hwnd: &Self::HWnd) {
        match (self.backend_and_wm(), &hwnd.inner) {
            (BackendAndWm::Native { wm }, HWndInner::Native(hwnd)) => {
                wm.update_wnd(hwnd);
            }
            (BackendAndWm::Testing { .. }, HWndInner::Testing(hwnd)) => {
                SCREEN.get_with_wm(self).update_wnd(hwnd);
            }
            _ => unreachable!(),
        }
    }

    fn get_wnd_size(self, hwnd: &Self::HWnd) -> [u32; 2] {
        match (self.backend_and_wm(), &hwnd.inner) {
            (BackendAndWm::Native { wm }, HWndInner::Native(hwnd)) => wm.get_wnd_size(hwnd),
            (BackendAndWm::Testing { .. }, HWndInner::Testing(hwnd)) => {
                SCREEN.get_with_wm(self).get_wnd_size(hwnd)
            }
            _ => unreachable!(),
        }
    }

    fn get_wnd_dpi_scale(self, hwnd: &Self::HWnd) -> f32 {
        match (self.backend_and_wm(), &hwnd.inner) {
            (BackendAndWm::Native { wm }, HWndInner::Native(hwnd)) => wm.get_wnd_dpi_scale(hwnd),
            (BackendAndWm::Testing { .. }, HWndInner::Testing(hwnd)) => {
                SCREEN.get_with_wm(self).get_wnd_dpi_scale(hwnd)
            }
            _ => unreachable!(),
        }
    }

    fn new_layer(self, attrs: LayerAttrs) -> Self::HLayer {
        match self.backend_and_wm() {
            BackendAndWm::Native { wm } => {
                let attrs = layer_attrs_to_native(attrs);
                HLayer {
                    inner: HLayerInner::Native(wm.new_layer(attrs)),
                }
            }
            BackendAndWm::Testing { .. } => {
                let attrs = layer_attrs_to_testing(attrs);
                let screen = SCREEN.get_with_wm(self);
                HLayer {
                    inner: HLayerInner::Testing(screen.new_layer(attrs)),
                }
            }
        }
    }
    fn set_layer_attr(self, hlayer: &Self::HLayer, attrs: LayerAttrs) {
        match (self.backend_and_wm(), &hlayer.inner) {
            (BackendAndWm::Native { wm }, HLayerInner::Native(hlayer)) => {
                let attrs = layer_attrs_to_native(attrs);
                wm.set_layer_attr(hlayer, attrs);
            }
            (BackendAndWm::Testing { .. }, HLayerInner::Testing(hlayer)) => {
                let attrs = layer_attrs_to_testing(attrs);
                SCREEN.get_with_wm(self).set_layer_attr(hlayer, attrs);
            }
            _ => unreachable!(),
        }
    }
    fn remove_layer(self, hlayer: &Self::HLayer) {
        match (self.backend_and_wm(), &hlayer.inner) {
            (BackendAndWm::Native { wm }, HLayerInner::Native(hlayer)) => {
                wm.remove_layer(hlayer);
            }
            (BackendAndWm::Testing { .. }, HLayerInner::Testing(hlayer)) => {
                SCREEN.get_with_wm(self).remove_layer(hlayer);
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HWnd {
    inner: HWndInner,
}

#[derive(Debug, Clone)]
enum HWndInner {
    Native(native::HWnd),
    Testing(screen::HWnd),
}

#[derive(Debug, Clone)]
pub struct HLayer {
    inner: HLayerInner,
}

#[derive(Debug, Clone)]
enum HLayerInner {
    Native(native::HLayer),
    Testing(screen::HLayer),
}

/// Convert `WndAttrs<'_>` to `native::WndAttrs<'_>`. Panics if some fields
/// are incompatible with the target backend.
fn wnd_attrs_to_native(attrs: WndAttrs<'_>) -> native::WndAttrs<'_> {
    let layer = attrs.layer.map(|layer_or_none| {
        layer_or_none.map(|hlayer| match hlayer.inner {
            HLayerInner::Native(hlayer) => hlayer,
            HLayerInner::Testing(_) => unreachable!(),
        })
    });
    native::WndAttrs {
        size: attrs.size,
        min_size: attrs.min_size,
        max_size: attrs.max_size,
        flags: attrs.flags,
        caption: attrs.caption,
        visible: attrs.visible,
        listener: attrs
            .listener
            .map(|listener| Box::new(wndlistenershim::NativeWndListener(listener)) as _),
        layer,
    }
}

/// Convert `WndAttrs<'_>` to `screen::WndAttrs<'_>`. Panics if some fields
/// are incompatible with the target backend.
fn wnd_attrs_to_testing(attrs: WndAttrs<'_>) -> screen::WndAttrs<'_> {
    let layer = attrs.layer.map(|layer_or_none| {
        layer_or_none.map(|hlayer| match hlayer.inner {
            HLayerInner::Native(_) => unreachable!(),
            HLayerInner::Testing(hlayer) => hlayer,
        })
    });
    screen::WndAttrs {
        size: attrs.size,
        min_size: attrs.min_size,
        max_size: attrs.max_size,
        flags: attrs.flags,
        caption: attrs.caption,
        visible: attrs.visible,
        listener: attrs.listener,
        layer,
    }
}

/// Convert `LayerAttrs` to `native::LayerAttrs`. Panics if some fields
/// are incompatible with the target backend.
fn layer_attrs_to_native(attrs: LayerAttrs) -> native::LayerAttrs {
    let sublayers = attrs.sublayers.map(|sublayers| {
        sublayers
            .into_iter()
            .map(|hlayer| match hlayer.inner {
                HLayerInner::Native(hlayer) => hlayer,
                HLayerInner::Testing(_) => unreachable!(),
            })
            .collect()
    });
    let contents = attrs.contents.map(|contents_or_none| {
        contents_or_none.map(|contents| match contents.inner {
            BitmapInner::Native(bitmap) => bitmap,
            BitmapInner::Testing(_) => panic!("Bitmap was created by the wrong backend"),
        })
    });
    native::LayerAttrs {
        transform: attrs.transform,
        contents,
        bounds: attrs.bounds,
        contents_center: attrs.contents_center,
        contents_scale: attrs.contents_scale,
        bg_color: attrs.bg_color,
        sublayers,
        opacity: attrs.opacity,
        flags: attrs.flags,
    }
}

/// Convert `LayerAttrs` to `screen::LayerAttrs`. Panics if some fields
/// are incompatible with the target backend.
fn layer_attrs_to_testing(attrs: LayerAttrs) -> screen::LayerAttrs {
    let sublayers = attrs.sublayers.map(|sublayers| {
        sublayers
            .into_iter()
            .map(|hlayer| match hlayer.inner {
                HLayerInner::Native(_) => unreachable!(),
                HLayerInner::Testing(hlayer) => hlayer,
            })
            .collect()
    });
    let contents = attrs.contents.map(|contents_or_none| {
        contents_or_none.map(|contents| match contents.inner {
            BitmapInner::Native(_) => panic!("Bitmap was created by the wrong backend"),
            BitmapInner::Testing(bitmap) => bitmap,
        })
    });
    screen::LayerAttrs {
        transform: attrs.transform,
        contents,
        bounds: attrs.bounds,
        contents_center: attrs.contents_center,
        contents_scale: attrs.contents_scale,
        bg_color: attrs.bg_color,
        sublayers,
        opacity: attrs.opacity,
        flags: attrs.flags,
    }
}

macro_rules! forward_args {
    ($expr:expr, $name:ident, self $( , $pname:ident: $t:ty )*) => { $expr.$name($($pname),*) };
    ($expr:expr, $name:ident, &self $( , $pname:ident: $t:ty )*) => { $expr.$name($($pname),*) };
    ($expr:expr, $name:ident, &mut self $( , $pname:ident: $t:ty )*) => { $expr.$name($($pname),*) };
}

/// `&mut self, pname: Ty, ...` → `&mut self.inner`
macro_rules! get_inner {
    (&mut $self:ident $($rest:tt)*) => {
        &mut $self.inner
    };
    (&$self:ident $($rest:tt)*) => {
        &$self.inner
    };
    ($self:ident $($rest:tt)*) => {
        $self.inner
    };
}

/// Forward methods to inner types. This macro is used for types like `Bitmap`
/// that there already is a type for each `Backend`.
macro_rules! forward {
    {
        inner_type: $inner_type:tt;
        fn $name:ident($($args:tt)*) $(-> $ret:ty)? ;
        $($rest:tt)*
    } => {
        fn $name($($args)*) $(-> $ret)? {
            match get_inner!($($args)*) {
                $inner_type::Native(inner) => forward_args!(inner, $name, $($args)*),
                $inner_type::Testing(inner) => forward_args!(inner, $name, $($args)*),
            }
        }
        forward! {
            inner_type: $inner_type;
            $($rest)*
        }
    };
    {
        inner_type: $inner_type:tt;
    } => {};
}

#[derive(Debug, Clone)]
pub struct Bitmap {
    inner: BitmapInner,
}

#[derive(Debug, Clone)]
enum BitmapInner {
    Native(native::Bitmap),
    Testing(bitmap::Bitmap),
}

impl iface::Bitmap for Bitmap {
    forward! {
        inner_type: BitmapInner;
        fn size(&self) -> [u32; 2];
    }
}

#[derive(Debug)]
pub struct BitmapBuilder {
    inner: BitmapBuilderInner,
}

#[derive(Debug)]
enum BitmapBuilderInner {
    Native(native::BitmapBuilder),
    Testing(bitmap::BitmapBuilder),
}

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        // Use the same backend as `Wm`
        match Wm::backend() {
            Backend::Native { .. } => Self {
                inner: BitmapBuilderInner::Native(native::BitmapBuilder::new(size)),
            },
            Backend::Testing { .. } => Self {
                inner: BitmapBuilderInner::Testing(bitmap::BitmapBuilder::new(size)),
            },
        }
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        match self.inner {
            BitmapBuilderInner::Native(bmp_builder) => Bitmap {
                inner: BitmapInner::Native(bmp_builder.into_bitmap()),
            },
            BitmapBuilderInner::Testing(bmp_builder) => Bitmap {
                inner: BitmapInner::Testing(bmp_builder.into_bitmap()),
            },
        }
    }
}

impl iface::Canvas for BitmapBuilder {
    forward! {
        inner_type: BitmapBuilderInner;
        fn save(&mut self);
        fn restore(&mut self);
        fn begin_path(&mut self);
        fn close_path(&mut self);
        fn move_to(&mut self, p: Point2<f32>);
        fn line_to(&mut self, p: Point2<f32>);
        fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>);
        fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>);
        fn fill(&mut self);
        fn stroke(&mut self);
        fn clip(&mut self);
        fn set_fill_rgb(&mut self, rgb: iface::RGBAF32);
        fn set_stroke_rgb(&mut self, rgb: iface::RGBAF32);
        fn set_line_cap(&mut self, cap: iface::LineCap);
        fn set_line_join(&mut self, join: iface::LineJoin);
        fn set_line_dash(&mut self, phase: f32, lengths: &[f32]);
        fn set_line_width(&mut self, width: f32);
        fn set_line_miter_limit(&mut self, miter_limit: f32);
        fn mult_transform(&mut self, m: Matrix3<f32>);
    }
}

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: iface::RGBAF32) {
        match (&mut self.inner, &layout.inner) {
            (BitmapBuilderInner::Native(bmp_builder), TextLayoutInner::Native(layout)) => {
                bmp_builder.draw_text(layout, origin, color)
            }
            (BitmapBuilderInner::Testing(bmp_builder), TextLayoutInner::Testing(layout)) => {
                bmp_builder.draw_text(layout, origin, color)
            }
            _ => panic!("Given BitmapBuilder and TextLayout belong to different backends"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CharStyle {
    inner: CharStyleInner,
}

#[derive(Debug, Clone)]
enum CharStyleInner {
    Native(native::CharStyle),
    Testing(text::CharStyle),
}

#[derive(Debug, Clone)]
enum OptionCharStyleInner {
    Native(Option<native::CharStyle>),
    Testing(Option<text::CharStyle>),
}

impl iface::CharStyle for CharStyle {
    fn new(attrs: CharStyleAttrs) -> Self {
        let template = if let Some(template) = attrs.template {
            match template.inner {
                CharStyleInner::Native(style) => OptionCharStyleInner::Native(Some(style)),
                CharStyleInner::Testing(style) => OptionCharStyleInner::Testing(Some(style)),
            }
        } else {
            // Use the same backend as `Wm`
            match Wm::backend() {
                Backend::Native { .. } => OptionCharStyleInner::Native(None),
                Backend::Testing { .. } => OptionCharStyleInner::Testing(None),
            }
        };

        match template {
            OptionCharStyleInner::Native(style) => Self {
                inner: CharStyleInner::Native(native::CharStyle::new(iface::CharStyleAttrs {
                    template: style,
                    sys: attrs.sys,
                    size: attrs.size,
                    decor: attrs.decor,
                    color: attrs.color,
                })),
            },
            OptionCharStyleInner::Testing(style) => Self {
                inner: CharStyleInner::Testing(text::CharStyle::new(iface::CharStyleAttrs {
                    template: style,
                    sys: attrs.sys,
                    size: attrs.size,
                    decor: attrs.decor,
                    color: attrs.color,
                })),
            },
        }
    }

    forward! {
        inner_type: CharStyleInner;
        fn size(&self) -> f32;
    }
}

#[derive(Debug)]
pub struct TextLayout {
    inner: TextLayoutInner,
}

#[derive(Debug)]
enum TextLayoutInner {
    Native(native::TextLayout),
    Testing(text::TextLayout),
}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        match &style.inner {
            CharStyleInner::Native(style) => Self {
                inner: TextLayoutInner::Native(native::TextLayout::from_text(text, style, width)),
            },
            CharStyleInner::Testing(style) => Self {
                inner: TextLayoutInner::Testing(text::TextLayout::from_text(text, style, width)),
            },
        }
    }

    forward! {
        inner_type: TextLayoutInner;
        fn visual_bounds(&self) -> Box2<f32>;
        fn layout_bounds(&self) -> Box2<f32>;
    }
}
