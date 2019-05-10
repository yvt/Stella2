//! Platform abstraction layer
use cfg_if::cfg_if;

pub mod iface;

/// Re-exports traits from `iface`.
pub mod prelude {
    pub use super::iface::{
        Bitmap, BitmapBuilder, BitmapBuilderNew, Canvas, CanvasText, CharStyle, TextLayout,
        WndListener, WM,
    };
}

// TODO: color management
//       Core Animation performs CPU-based color matching if the color profile
//       of images doesn't match that of the display. This overhead can be
//       addressed by assigning a correct profile on images.

cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub mod macos;

        /// The default window manager type for the target platform.
        pub type WM = macos::WM;

        /// The default bitmap type for the target platform implementing
        /// `Bitmap`.
        pub type Bitmap = macos::Bitmap;

        /// The default bitmap builder type for the target platform implementing
        /// `BitmapBuilderNew` and `CanvasText<TextLayout>`.
        pub type BitmapBuilder = macos::BitmapBuilder;

        /// The default character style type for the target platform
        /// implementing `CharStyle`.
        pub type CharStyle = macos::CharStyle;

        /// The default text layout type for the target platform
        /// implementing `TextLayout`.
        pub type TextLayout = macos::TextLayout;
    }
    // TODO: Other platforms
}

// ============================================================================
//
// Type aliases/re-exports from `iface` with concrete backend types are
// defined below.
//
// Implementation notes: It's okay to use the following types in the backend
// code. In other words, enabled backends can assume that they are the default
// backend.

pub use self::iface::{
    LayerFlags, LineCap, LineJoin, SysFontType, TextDecorFlags, WndFlags, RGBAF32,
};

/// The window handle type of [`WM`].
pub type HWnd = <WM as iface::WM>::HWnd;

/// The layer handle type of [`WM`].
pub type HLayer = <WM as iface::WM>::HLayer;

/// A specialization of `WndAttrs` for the default backend.
pub type WndAttrs<'a> = iface::WndAttrs<'a, WM, HLayer>;

/// A specialization of `LayerAttrs` for the default backend.
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;

/// A specialization of `CharStyleAttrs` for the default backend.
pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

// Trait aliases (unstable at the point of writing) actually do not work
// exactly like type aliases. Specifically, they cannot be used in every place
// where traits can be used, like `impl` blocks.
//
//      pub trait WndListener = iface::WndListener<WM>;
//
