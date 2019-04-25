use super::types::WndAttrs;

pub trait WM: Sized {
    /// A window handle type.
    type HWnd: Send + Sync + Clone;

    fn enter_main_loop(&self);
    fn terminate(&self);

    fn new_wnd(&self, attrs: &WndAttrs<Self, &str>) -> Self::HWnd;

    /// Set the attributes of a window.
    ///
    /// Panics if the window has already been closed.
    fn set_wnd_attr(&self, window: &Self::HWnd, attrs: &WndAttrs<Self, &str>);
    fn remove_wnd(&self, window: &Self::HWnd);
}

/// Window event handlers.
///
/// The receiver is immutable because event handlers may manipulate windows,
/// which in turn might cause other event handlers to be called.
pub trait WndListener<T: WM> {
    /// The user has attempted to close a window. Returns `true` if the window
    /// can be closed.
    fn close_requested(&self, _: &T, _: &T::HWnd) -> bool {
        true
    }

    /// A window has been closed.
    fn close(&self, _: &T, _: &T::HWnd) {}

    // TODO: more events
}

/// A immutable, ref-counted bitmap image.
pub trait Bitmap: Clone + Sized {
    // TODO
}

/// Types supporting drawing operations.
pub trait Canvas {
    // TODO
}

/// A builder type for [`Bitmap`] supporting 2D drawing operations via
/// [`Canvas`].
pub trait BitmapBuilder: Canvas {
    type Bitmap: Bitmap;

    fn into_bitmap(self) -> Self::Bitmap;
}

pub trait BitmapBuilderNew: BitmapBuilder + Sized {
    /// Create a [`BitmapBuilder`] with a R8G8B8A8 backing bitmap.
    fn new(size: [u32; 2]) -> Self;
}
