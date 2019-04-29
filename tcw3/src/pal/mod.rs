//! Platform abstraction layer
use cfg_if::cfg_if;

pub mod iface;

/// Re-exports traits from `iface`.
pub mod prelude {
    pub use super::iface::{Bitmap, BitmapBuilder, BitmapBuilderNew, Canvas, WndListener, WM};
}

cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub mod macos;

        /// The default window manager type for the target platform.
        pub type WM = macos::WM;

        /// The default bitmap type for the target platform implementing
        /// `Bitmap`.
        pub type Bitmap = macos::Bitmap;

        /// The default bitmap builder type for the target platform implementing
        /// `BitmapBuilderNew`.
        pub type BitmapBuilder = macos::BitmapBuilder;
    }
    // TODO: Other platforms
}

/// Get the default instance of [`WM`]. It only can be called by a main thread.
#[inline]
pub fn wm() -> &'static WM {
    WM::global()
}

// ============================================================================
//
// Type aliases/re-exports from `iface` with concrete backend types are
// defined below.
//
// Implementation notes: It's okay to use the following types in the backend
// code. In other words, enabled backends can assume that they are the default
// backend.

pub use self::iface::{LayerFlags, LineCap, LineJoin, WndFlags, RGBAF32};

/// The window handle type of [`WM`].
pub type HWnd = <WM as iface::WM>::HWnd;

/// The layer handle type of [`WM`].
pub type HLayer = <WM as iface::WM>::HLayer;

/// A specialization of `WndAttrs` for the default backend.
pub type WndAttrs<TCaption> = iface::WndAttrs<WM, TCaption, HLayer>;

/// A specialization of `LayerAttrs` for the default backend.
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;

// Trait aliases (unstable at the point of writing) actually do not work
// exactly like type aliases. Specifically, they cannot be used in every place
// where traits can be used, like `impl` blocks.
//
//      pub trait WndListener = iface::WndListener<WM>;
//
