//! Platform abstraction layer
use cfg_if::cfg_if;

pub mod traits;
pub mod types;

#[doc(no_inline)]
pub use self::types::*;

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

/// The window handle type of [`WM`].
pub type HWnd = <WM as traits::WM>::HWnd;
