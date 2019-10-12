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
use super::iface;
use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use std::marker::PhantomData;

mod wmapi;
pub use self::wmapi::TestingWm;

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

// Borrow some modules from `unix` backend
#[path = "unix/bitmap.rs"]
mod bitmap;
#[path = "unix/text.rs"]
mod text;

// The following items are all TODO

/// Activate the testing backend and call the given function on the main thread.
///
/// Panics if the native backend has already been initialized.
/// See [the module documentation](index.html) for more.
pub fn with_testing_wm<R: Send>(_cb: impl FnOnce(Wm) -> R + Send) -> R {
    unimplemented!()
}

/// Call `with_testing_wm` if the testing backend is enabled. Otherwise,
/// output a warning message and return without calling the givne function.
///
/// This function is available even if the `testing` feature flag is disabled.
pub fn run_test(cb: impl FnOnce(&dyn TestingWm) + Send) {
    with_testing_wm(|wm| cb(&wm));
}

// TODO: Add artificial inputs and outputs

#[derive(Debug, Clone, Copy)]
pub struct Wm {
    _no_send_sync: std::marker::PhantomData<*mut ()>,
}

impl wmapi::TestingWm for Wm {
    fn wm(&self) -> crate::Wm {
        *self
    }

    fn step_until(&self, till: std::time::Instant) {
        std::thread::sleep(till.saturating_duration_since(std::time::Instant::now()));
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
        unimplemented!()
    }

    fn invoke_on_main_thread(f: impl FnOnce(Wm) + Send + 'static) {
        unimplemented!()
    }

    fn invoke(self, f: impl FnOnce(Self) + 'static) {
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

#[derive(Debug, Clone)]
pub struct HWnd;

#[derive(Debug, Clone)]
pub struct HLayer;

#[derive(Debug, Clone)]
pub struct Bitmap;

impl iface::Bitmap for Bitmap {
    fn size(&self) -> [u32; 2] {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct BitmapBuilder;

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        unimplemented!()
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        unimplemented!()
    }
}

impl iface::Canvas for BitmapBuilder {
    fn save(&mut self) {
        unimplemented!()
    }
    fn restore(&mut self) {
        unimplemented!()
    }
    fn begin_path(&mut self) {
        unimplemented!()
    }
    fn close_path(&mut self) {
        unimplemented!()
    }
    fn move_to(&mut self, p: Point2<f32>) {
        unimplemented!()
    }
    fn line_to(&mut self, p: Point2<f32>) {
        unimplemented!()
    }
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>) {
        unimplemented!()
    }
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>) {
        unimplemented!()
    }
    fn fill(&mut self) {
        unimplemented!()
    }
    fn stroke(&mut self) {
        unimplemented!()
    }
    fn clip(&mut self) {
        unimplemented!()
    }
    fn set_fill_rgb(&mut self, rgb: iface::RGBAF32) {
        unimplemented!()
    }
    fn set_stroke_rgb(&mut self, rgb: iface::RGBAF32) {
        unimplemented!()
    }
    fn set_line_cap(&mut self, cap: iface::LineCap) {
        unimplemented!()
    }
    fn set_line_join(&mut self, join: iface::LineJoin) {
        unimplemented!()
    }
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]) {
        unimplemented!()
    }
    fn set_line_width(&mut self, width: f32) {
        unimplemented!()
    }
    fn set_line_miter_limit(&mut self, miter_limit: f32) {
        unimplemented!()
    }
    fn mult_transform(&mut self, m: Matrix3<f32>) {
        unimplemented!()
    }
}

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: iface::RGBAF32) {
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub struct CharStyle;

impl iface::CharStyle for CharStyle {
    fn new(attrs: CharStyleAttrs) -> Self {
        unimplemented!()
    }

    fn size(&self) -> f32 {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct TextLayout;

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        unimplemented!()
    }

    fn visual_bounds(&self) -> Box2<f32> {
        unimplemented!()
    }

    fn layout_bounds(&self) -> Box2<f32> {
        unimplemented!()
    }
}
