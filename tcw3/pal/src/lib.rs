//! TCW3 Platform abstraction layer
//!
//! This crate is reexported by TCW3 as `tcw3::pal`.
#![feature(const_fn)] // `'static` bounds on `const fn` parameters
#![feature(is_sorted)] // `<[_]>::is_sorted`
#![feature(unsized_locals)] // Call `dyn FnOnce`
#![allow(clippy::float_cmp)]
// this lint is ridiculous
// When never type (`!`) is stabilized, `msg_send![ ... ];` will be no longer
// deduced to `()`. Thus a call to `msg_send!` needs a unit value binding
#![allow(clippy::let_unit_value)]

mod canvas;
pub mod futuresext;
pub mod iface;

/// Re-exports traits from `iface`.
///
/// The trait [`Wm`](crate::iface::Wm) is re-exported as `WmTrait` so that it
/// doesn't collide with the concrete type alias [`Wm`](crate::Wm).
pub mod prelude {
    pub use super::cells::MtLazyStatic;
    pub use super::iface::{
        Bitmap, BitmapBuilder, BitmapBuilderNew, Canvas, CanvasText, CharStyle, MouseDragListener,
        ScrollListener, TextInputCtxEdit, TextInputCtxListener, TextLayout, Wm as WmTrait,
        WndListener,
    };

    pub use super::futuresext::WmFuturesExt;
}

// TODO: Color theme

// TODO: color management
//       Core Animation performs CPU-based color matching if the color profile
//       of images doesn't match that of the display. This overhead can be
//       addressed by assigning a correct profile on images.

// ============================================================================
//
// Utilities (should be defined first because it defines a macro
// used by some submodules)
//
#[macro_use]
mod cells;
pub use self::cells::{MtLock, MtSticky};

// This procedural macro can be used by backend implementations, so must
// precede `mod macos`, etc.
//
// `#[proc_macro_hack]` makes it possible to use this procedural macro in
// expression position without relying on an unstable rustc feature, but with
// some restrictions. See `proc_macro_hack`'s documentation for more.
#[doc(hidden)]
#[proc_macro_hack::proc_macro_hack]
pub use tcw3_pal_macro::accel_table_inner;

// ============================================================================
//
// The main module for each target platform. The active one for the current
// target is aliased as `native`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos as native;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows as native;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub mod gtk;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use self::gtk as native;

#[cfg(any(
    not(any(target_os = "macos", target_os = "windows")),
    feature = "testing"
))]
mod swrast;

#[cfg(feature = "testing")]
mod timerqueue;

// ============================================================================
//
// If the testing backend is enabled, it wraps and replaces the default native
// backend.

#[cfg(feature = "testing")]
pub mod testing;

#[cfg(not(feature = "testing"))]
#[path = "testing_dis.rs"]
pub mod testing;

#[cfg(feature = "testing")]
pub use self::testing as current;

#[cfg(not(feature = "testing"))]
pub use self::native as current;

// ============================================================================
//
// Define `accel_table`. This is trickier than other items because macros
// only have crate-level namespaces.

// The implementation of `native_accel_table` is chosen based on the target
// backend.
#[doc(hidden)]
#[macro_export]
#[cfg(target_os = "macos")]
macro_rules! native_accel_table {
    ($($entries:tt)*) => {
        $crate::accel_table_inner!($crate, "macos", [$($entries)*])
    };
}

#[doc(hidden)]
#[macro_export]
#[cfg(target_os = "windows")]
macro_rules! native_accel_table {
    ($($entries:tt)*) => {
        $crate::accel_table_inner!($crate, "windows", [$($entries)*])
    };
}

#[doc(hidden)]
#[macro_export]
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
macro_rules! native_accel_table {
    ($($entries:tt)*) => {
        $crate::accel_table_inner!($crate, "gtk", [$($entries)*])
    };
}

// Finally, choose the implementation of `accel_table` based on whether
// `testing` is enabled.

macro_rules! define_accel_table {
    ($($inner:tt)*) => {
        /// Create an accelerator table. Expands to a constant expression of
        /// type [`AccelTable`].
        ///
        /// # Example
        ///
        /// ```
        /// use tcw3_pal::{accel_table, AccelTable, actions};
        /// static ACCEL_TABLE: AccelTable = accel_table![
        ///     (actions::COPY, windows("Ctrl+C"), macos("Super+C"), gtk("Ctrl+C"),
        ///         macos_sel("copy:")),
        /// ];
        /// ```
        #[macro_export]
        $($inner)*
    };
}

define_accel_table! {
    #[cfg(not(feature = "testing"))]
    macro_rules! accel_table {
        ($($entries:tt)*) => {
            $crate::native_accel_table!($($entries)*)
        };
    }
}

define_accel_table! {
    #[cfg(feature = "testing")]
    macro_rules! accel_table {
        ($($entries:tt)*) => {
            $crate::testing::AccelTable::__new(
                $crate::accel_table_inner!($crate, "testing", [$($entries)*]),
                $crate::native_accel_table!($($entries)*),
            )
        };
    }
}

// ============================================================================
//
// Type aliases for the default backend.

/// The default window manager type for the target platform.
pub type Wm = current::Wm;

/// The default bitmap type for the target platform implementing
/// `Bitmap`.
pub type Bitmap = current::Bitmap;

/// The default bitmap builder type for the target platform implementing
/// `BitmapBuilderNew` and `CanvasText<TextLayout>`.
pub type BitmapBuilder = current::BitmapBuilder;

/// The default character style type for the target platform
/// implementing `CharStyle`.
pub type CharStyle = current::CharStyle;

/// The default text layout type for the target platform
/// implementing `TextLayout`.
pub type TextLayout = current::TextLayout;

// ============================================================================
//
// Type aliases/re-exports from `iface` specialized for the default backend are
// defined below.
//
// Implementation notes: It's *not* okay to use the following types in the
// backend code. In other words, enabled backends must not assume that they are
// the default backend.

pub use self::iface::{
    actions, ActionId, ActionStatus, BadThread, Beam, CursorShape, IndexFromPointFlags,
    InterpretEventCtx, LayerFlags, LineCap, LineJoin, NcHit, RunFlags, RunMetrics, ScrollDelta,
    SysFontType, TextDecorFlags, TextInputCtxEventFlags, WndFlags, RGBAF32,
};

/// The window handle type of [`Wm`].
pub type HWnd = <Wm as iface::Wm>::HWnd;

/// The layer handle type of [`Wm`].
pub type HLayer = <Wm as iface::Wm>::HLayer;

/// The invocation handle type of [`Wm`].
pub type HInvoke = <Wm as iface::Wm>::HInvoke;

/// The accelerator table handle type of [`Wm`].
pub type AccelTable = <Wm as iface::Wm>::AccelTable;

/// The text input context handle type of [`Wm`].
pub type HTextInputCtx = <Wm as iface::Wm>::HTextInputCtx;

/// A specialization of `WndAttrs` for the default backend.
pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;

/// A specialization of `LayerAttrs` for the default backend.
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;

/// A specialization of `CharStyleAttrs` for the default backend.
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

// Trait aliases (unstable at the point of writing) actually do not work
// exactly like type aliases. Specifically, they cannot be used in every place
// where traits can be used, like `impl` blocks.
//
//      pub trait WndListener = iface::WndListener<Wm>;
//
