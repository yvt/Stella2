//! TCW3 Platform abstraction layer
//!
//! This crate is reexported by TCW3 as `tcw3::pal`.
#![feature(const_fn)] // `'static` bounds on `const fn` parameters
#![feature(doc_cfg)] // `cfg(rustdoc)`
#![feature(is_sorted)] // `<[_]>::is_sorted`

mod canvas;
pub mod iface;

/// Re-exports traits from `iface`.
pub mod prelude {
    pub use super::cells::MtLazyStatic;
    pub use super::iface::{
        Bitmap, BitmapBuilder, BitmapBuilderNew, Canvas, CanvasText, CharStyle, MouseDragListener,
        TextLayout, Wm, WndListener,
    };
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

// ============================================================================
//
// The main module for each target platform. The active one for the current
// target is aliased as `native`.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos as native;

#[cfg(not(target_os = "macos"))]
pub mod unix;
#[cfg(not(target_os = "macos"))]
pub use unix as native;

// TODO: Windows

// And here is the supporting module, which is shared between the
// platform-specific modules.
#[cfg(feature = "winit")]
mod winit;

#[cfg(not(target_os = "macos"))]
mod swrast;

// ============================================================================
//
// Type aliases for the default backend.

// TODO: A test driver, which replaces the following type aliases, allowing
//       UI tests to provide stimuli

/// The default window manager type for the target platform.
pub type Wm = native::Wm;

/// The default bitmap type for the target platform implementing
/// `Bitmap`.
pub type Bitmap = native::Bitmap;

/// The default bitmap builder type for the target platform implementing
/// `BitmapBuilderNew` and `CanvasText<TextLayout>`.
pub type BitmapBuilder = native::BitmapBuilder;

/// The default character style type for the target platform
/// implementing `CharStyle`.
pub type CharStyle = native::CharStyle;

/// The default text layout type for the target platform
/// implementing `TextLayout`.
pub type TextLayout = native::TextLayout;

// ============================================================================
//
// Type aliases/re-exports from `iface` specialized for the default backend are
// defined below.
//
// Implementation notes: It's okay to use the following types in the backend
// code. In other words, enabled backends can assume that they are the default
// backend. TODO: This will be no longer true once we have a test driver

pub use self::iface::{
    BadThread, LayerFlags, LineCap, LineJoin, SysFontType, TextDecorFlags, WndFlags, RGBAF32,
};

/// The window handle type of [`Wm`].
pub type HWnd = <Wm as iface::Wm>::HWnd;

/// The layer handle type of [`Wm`].
pub type HLayer = <Wm as iface::Wm>::HLayer;

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
