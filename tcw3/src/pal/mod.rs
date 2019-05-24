//! Platform abstraction layer
use cfg_if::cfg_if;

pub mod iface;

/// Re-exports traits from `iface`.
pub mod prelude {
    pub use super::iface::{
        Bitmap, BitmapBuilder, BitmapBuilderNew, Canvas, CanvasText, CharStyle, MouseDragListener,
        TextLayout, WndListener, WM,
    };
}

// TODO: Color theme

// TODO: color management
//       Core Animation performs CPU-based color matching if the color profile
//       of images doesn't match that of the display. This overhead can be
//       addressed by assigning a correct profile on images.

#[cfg(target_os = "macos")]
pub mod macos;

/// The type aliases for the current target platform.
pub mod native {
    use super::*;

    cfg_if! {
        if #[cfg(target_os = "macos")] {
            pub type WM = macos::WM;
            pub type Bitmap = macos::Bitmap;
            pub type BitmapBuilder = macos::BitmapBuilder;
            pub type CharStyle = macos::CharStyle;
            pub type TextLayout = macos::TextLayout;
        }
    }
}

// TODO: Other platforms

// TODO: A test driver, which replaces the following type aliases, allowing
//       UI tests to provide stimuli

/// The default window manager type for the target platform.
pub type WM = native::WM;

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
