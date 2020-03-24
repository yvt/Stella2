//! Defines an abstract interface to the backend implementation.
//!
//! This module defines an abstract interface not bound to any specific types
//! defined in the backend implementation.
//!
//! The parent module (`pal`) provides type aliases for the types defined here,
//! specialized for the default backend, as well as simple re-exports of
//! non-generic types.
use bitflags::bitflags;
use cggeom::{box2, Box2};
use cgmath::{Matrix3, Point2, Vector2};
use rgb::RGBA;
use std::{borrow::Cow, fmt, fmt::Debug, hash::Hash, ops::Range, time::Duration};

pub type RGBAF32 = RGBA<f32>;

// FIXME: Our API might not be a perfect fit for some platforms. This is because
//        the API was originally built around Cocoa (macOS's system API). It's
//        perfectly okay to modify it.

/// A trait for window managers.
///
/// All methods are reentrant with some exceptions.
pub trait Wm: Clone + Copy + Sized + Debug + 'static {
    /// A window handle type.
    type HWnd: Debug + Clone + PartialEq + Eq + Hash;

    /// A layer handle type.
    ///
    /// A layer only can appear in a single window throughout its lifetime.
    /// I.e., after a layer is added to a window, it must never moved to another
    /// window.
    type HLayer: Debug + Clone + PartialEq + Eq + Hash;

    /// Represents a function call pended by `invoke_after`.
    type HInvoke: Debug + Clone + PartialEq + Eq + Hash + Send + Sync;

    /// A text input context handle type.
    type HTextInputCtx: Debug + Clone + PartialEq + Eq + Hash;

    /// A bitmap type.
    type Bitmap: Bitmap;

    /// Get the default instance of [`Wm`]. It only can be called by a main thread.
    fn global() -> Self {
        Self::try_global().unwrap()
    }

    /// Get the default instance of [`Wm`] without checking the calling thread.
    ///
    /// # Safety
    ///
    /// The calling thread should be a main thread, i.e., the thread
    /// wherein `Self::is_main_thread()` returns `true`.
    unsafe fn global_unchecked() -> Self;

    fn try_global() -> Result<Self, BadThread> {
        if Self::is_main_thread() {
            Ok(unsafe { Self::global_unchecked() })
        } else {
            Err(BadThread)
        }
    }

    /// Check if the calling thread is the main thread or not.
    ///
    /// On some backends, the first thread calling this method is registered as
    /// the main thread. On other backends, the first thread created in the
    /// process is always recognized as the main thread.
    fn is_main_thread() -> bool;

    /// Enqueue a call to the specified function on the main thread. The calling
    /// thread can be any thread.
    ///
    /// This method may panic if it is called before a main thread is
    /// determined.
    fn invoke_on_main_thread(f: impl FnOnce(Self) + Send + 'static);

    /// Enqueue a call to the specified function on the main thread.
    fn invoke(self, f: impl FnOnce(Self) + 'static);

    /// Enqueue a call to the specified function on the main thread after the
    /// specified delay.
    ///
    /// The delay is specified as a range. The lower bound (`delay.start`) is
    /// the default delay. To optimize power usage, the system may choose to
    /// adjust the delay in the specified range.
    ///
    /// The implementations may set a hard limit on the number of pending calls.
    /// An attempt to surpass the limit causes a panic. The lower bound of the
    /// limit is currently `64` (the hard-coded limit of `TimerQueue`).
    ///
    /// The delay must be shorter than 2³⁰ milliseconds.
    fn invoke_after(self, delay: Range<Duration>, f: impl FnOnce(Self) + 'static) -> Self::HInvoke;

    /// Cancel a pending function call enqueued by `invoke_after`. Does nothing
    /// if the function was already called or is being called. Otherwise, the
    /// associated function will never be called.
    fn cancel_invoke(self, hinv: &Self::HInvoke);

    /// Enter the main loop. This method will never return.
    ///
    /// It's not allowed to call this method from a `WndListener`.
    fn enter_main_loop(self) -> !;

    /// Quit the application gracefully.
    fn terminate(self);

    /// Create a layer.
    fn new_wnd(self, attrs: WndAttrs<'_, Self, Self::HLayer>) -> Self::HWnd;

    /// Set the attributes of a window.
    ///
    /// Panics if the window has already been closed. Also, it's not allowed to
    /// replace a window's `WndListener` while a method of the current one is
    /// currently being called.
    fn set_wnd_attr(self, window: &Self::HWnd, attrs: WndAttrs<'_, Self, Self::HLayer>);

    /// Destroy a window.
    ///
    /// The window will be closed as soon as possible (if not immediately).
    /// `WndListener::close_requested` is not called. All system resources
    /// associated with the window will be released.
    ///
    /// All text input contexts (represented by `Self::HTextInputCtx`)
    /// associated with the window will be invalidated, and the further uses of
    /// the contexts through `Wm`'s methods except `remove_text_input_ctx` may
    /// cause a panic.
    fn remove_wnd(self, window: &Self::HWnd);

    /// Update a window's contents.
    ///
    /// Calling this method requests that the contents of a window is updated
    /// based on the attributes of the window and its all sublayers as soon as
    /// possible. Conversely, all attribute updates may be deferred until this
    /// method is called.
    ///
    /// The implementation **may** call this automatically in the main event
    /// loop, but the client must not assume that this will happen.
    fn update_wnd(self, window: &Self::HWnd);

    /// Request to have [`WndListener::update_ready`] called when the
    /// window is ready to accept a new update.
    ///
    /// The client may use this method to meter the update of a window in order
    /// that it does not generate more frames than necessary, but is not
    /// required to use this.
    ///
    /// The implementation of `WndListener::resize` **must not** use this method
    /// to defer the update.
    fn request_update_ready_wnd(self, window: &Self::HWnd);

    /// Get the size of a window's content region.
    fn get_wnd_size(self, window: &Self::HWnd) -> [u32; 2];

    /// Get the DPI scaling factor of a window.
    fn get_wnd_dpi_scale(self, _window: &Self::HWnd) -> f32 {
        1.0
    }

    /// Get a flag indicating whether the specified window has focus.
    fn is_wnd_focused(self, window: &Self::HWnd) -> bool;

    /// Create a layer.
    fn new_layer(self, attrs: LayerAttrs<Self::Bitmap, Self::HLayer>) -> Self::HLayer;

    /// Set the attributes of a layer.
    ///
    /// The behavior is unspecified if the layer has already been removed.
    fn set_layer_attr(self, layer: &Self::HLayer, attrs: LayerAttrs<Self::Bitmap, Self::HLayer>);

    /// Delete a layer.
    ///
    /// If the layer has a superlayer, the deletion will be postponed until it's
    /// removed from the superlayer or the superlayer is deleted. Thus, it's
    /// safe to call this method for a layer still in use.
    fn remove_layer(self, layer: &Self::HLayer);

    /// Create a text input context.
    fn new_text_input_ctx(
        self,
        hwnd: &Self::HWnd,
        listener: Box<dyn TextInputCtxListener<Self>>,
    ) -> Self::HTextInputCtx;

    /// Notify that the text document associated with the given text input
    /// context was updated and the input service should discard any ongoing
    /// conversion session.
    ///
    /// This must not called in response to a call to `TextInputCtxEdit`'s
    /// method.
    fn text_input_ctx_reset(self, _: &Self::HTextInputCtx) {}

    /// Notify that the selection range of the given text input context has
    /// changed.
    ///
    /// This must not called in response to a call to `TextInputCtxEdit`'s
    /// method.
    ///
    /// This method may call [`TextInputCtxListener::edit`].
    fn text_input_ctx_on_selection_change(self, _: &Self::HTextInputCtx) {}

    /// Notify that the layout (e.g., the caret position with reference to the
    /// window) of the given text input context has changed.
    ///
    /// This must not called in response to a call to `TextInputCtxEdit`'s
    /// method.
    ///
    /// This method may call [`TextInputCtxListener::edit`].
    fn text_input_ctx_on_layout_change(self, _: &Self::HTextInputCtx) {}

    /// Activate or deactivate the specified text input context.
    ///
    /// In an application process, there can be only once active text input
    /// context. When multiple contexts are activated, only one of them will be
    /// activated, but how that will be chosen is unspecified.
    ///
    /// This method may call [`TextInputCtxListener`]`::{edit, set_event_flag}`.
    fn text_input_ctx_set_active(self, _: &Self::HTextInputCtx, active: bool);

    /// Delete the specified text input context.
    ///
    /// [`TextInputCtxListener::edit`] may be called in this method.
    fn remove_text_input_ctx(self, ctx: &Self::HTextInputCtx);
}

/// Returned when a function/method is called from an invalid thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BadThread;

impl std::fmt::Display for BadThread {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "the operation is invalid for the current thread")
    }
}

impl std::error::Error for BadThread {}

#[allow(clippy::option_option)] // for consistency between fields
pub struct WndAttrs<'a, T: Wm, TLayer> {
    /// The size of the content region.
    pub size: Option<[u32; 2]>,
    pub min_size: Option<[u32; 2]>,
    pub max_size: Option<[u32; 2]>,
    pub flags: Option<WndFlags>,
    pub caption: Option<Cow<'a, str>>,
    pub visible: Option<bool>,
    pub listener: Option<Box<dyn WndListener<T>>>,
    pub layer: Option<Option<TLayer>>,
    pub cursor_shape: Option<CursorShape>,
}

impl<'a, T: Wm, TLayer> Default for WndAttrs<'a, T, TLayer> {
    fn default() -> Self {
        Self {
            size: None,
            min_size: None,
            max_size: None,
            flags: None,
            caption: None,
            visible: None,
            listener: None,
            layer: None,
            cursor_shape: None,
        }
    }
}

bitflags! {
    pub struct WndFlags: u32 {
        const RESIZABLE = 1;
        const BORDERLESS = 1 << 1;

        /// Makes the window background transparent and enables the "blur
        /// behind" effect if supported by the system.
        ///
        /// In general, every pixel of the window must be covered by fully
        /// opaque layer contents (including the background color). If this
        /// flag is set, layers with a `BACKDROP_BLUR` flag also count as
        /// opaque contents (even if they don't have actual contents).
        const TRANSPARENT_BACKDROP_BLUR = 1 << 2;
    }
}

impl Default for WndFlags {
    fn default() -> Self {
        WndFlags::RESIZABLE
    }
}

impl<T: Wm, TLayer: Debug> Debug for WndAttrs<'_, T, TLayer> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WndAttrs")
            .field("size", &self.size)
            .field("min_size", &self.min_size)
            .field("max_size", &self.max_size)
            .field("flags", &self.flags)
            .field("caption", &self.caption)
            .field("visible", &self.visible)
            .field(
                "listener",
                &self.listener.as_ref().map(|bx| (&*bx) as *const _),
            )
            .field("layer", &self.layer)
            .finish()
    }
}

#[cfg_attr(doc, svgbobdoc::transform)]
/// Specifies layer attributes.
#[allow(clippy::option_option)] // for consistency between fields
#[derive(Debug, Clone)]
pub struct LayerAttrs<TBitmap, TLayer> {
    /// The 2D transformation applied to the contents of the layer.
    /// It doesn't have an effect on sublayers.
    ///
    /// The input coordinate space is the one used to express `bounds`. The
    /// output coordinate space is virtual pixel coordinates with `(0,0)` at the
    /// top left corner of a window's client region.
    ///
    /// `value.x.z` and `value.y.z` may be assumed to be zero. This means
    /// projective transformations are not supported and only affine
    /// transformations can be expressed. `value.z.z` must be positive.
    pub transform: Option<Matrix3<f32>>,

    /// Specifies the content image of the layer.
    pub contents: Option<Option<TBitmap>>,
    /// Specifies the bounds of the content image.
    ///
    /// Because of how the anchor point is calculated in the macOS bakcend, it
    /// must not be empty.
    pub bounds: Option<Box2<f32>>,
    /// Specifies the flexible region of the content image.
    ///
    /// It defaults to `(0,0)-(1,1)`, indicating entire the image is scaled in
    /// both directions to match the content bounds. When set to a non-default
    /// value, the content image is split into 3×3 slices. The four corner
    /// slices do not scale and the four edge slices only scale along their
    /// corresponding edges, while only the central slice scales freely.
    /// `contents_center` specifies the location of the central slice within the
    /// source image. This is commonly referred to as [*9-slice scaling*].
    ///
    /// [*9-slice scaling*]: https://en.wikipedia.org/wiki/9-slice_scaling
    ///
    /// ```svgbob
    ///                                                              ,--+-------------+--,
    /// (0,0)  min  max                                              |A |             | B|
    ///      *--*----*--,                           ,--+----+--,     +--+-------------+--+
    ///      |A |    | B|            ,--+--+--,     |A |    | B|     |  |      .      |  |
    ///  min *--+----+--+            |A |  | B|     +--+----+--+     |  |     / \     |  |
    ///      |  | △  |  |    --->    +--+--+--+     |  | △  |  |     |  |    /   \    |  |
    ///  max *--+----+--+            |D |  | C|     +--+----+--+     |  |   /     \   |  |
    ///      |D |    | C|            '--+--+--'     |D |    | C|     |  |  +-------+  |  |
    ///      '--+----+--*                           '--+----+--'     +--+-------------+--+
    ///                  (1,1)                                       |D |             | C|
    ///                                                              '--+-------------+--'
    /// ```
    pub contents_center: Option<Box2<f32>>,
    /// Specifies the natural scaling ratio of the content image.
    ///
    /// This is used only when `contents_center` has a non-default value.
    /// Defaults to `1.0`.
    pub contents_scale: Option<f32>,
    /// Specifies the solid color underlaid to the content image.
    pub bg_color: Option<RGBAF32>,

    pub sublayers: Option<Vec<TLayer>>,

    /// Specifies the opacity value.
    ///
    /// Defaults to `1.0`. Sublayers are affected as well. The opacity value
    /// is applied after the sublayers are composited, thus it has a different
    /// effect than applying the value on the sublayers individually.
    pub opacity: Option<f32>,

    /// Specifies additional options on the layer.
    pub flags: Option<LayerFlags>,
}

impl<TBitmap, TLayer> LayerAttrs<TBitmap, TLayer> {
    /// Replace the fields with values from `o` if they are `Some(_)`.
    pub fn override_with(&mut self, o: Self) {
        macro_rules! process_one {
            ($i:ident) => {
                if let Some(x) = o.$i {
                    self.$i = Some(x);
                }
            };
        }
        process_one!(transform);
        process_one!(contents);
        process_one!(bounds);
        process_one!(contents_center);
        process_one!(contents_scale);
        process_one!(bg_color);
        process_one!(sublayers);
        process_one!(opacity);
        process_one!(flags);
    }
}

impl<TBitmap, TLayer> Default for LayerAttrs<TBitmap, TLayer> {
    fn default() -> Self {
        Self {
            transform: None,
            contents: None,
            bounds: None,
            contents_center: None,
            contents_scale: None,
            sublayers: None,
            bg_color: None,
            opacity: None,
            flags: None,
        }
    }
}

bitflags! {
    pub struct LayerFlags: u32 {
        /// Clip sublayers to the content bounds.
        ///
        /// **This flag cannot be modified once a layer is created.** Changing
        /// it via `set_layer_attr` might cause visual corruption on some
        /// backends (namely, `swrast`).
        const MASK_TO_BOUNDS = 1;

        /// Draw the "blur behind" effect behind the layer.
        ///
        /// The following condition must be upheld for this flag to work in
        /// a consistent way:
        ///
        ///  - There must be no layer behind this layer. (Some backends might
        ///    blur the contents behind the window while the others use the
        ///    contents behind the layer.)
        ///  - The containing window has a `TRANSPARENT_BACKDROP_BLUR` flag.
        ///  - The region occupied by the layer must be an axis-aligned
        ///    rectangular region.
        ///  - The layer's transformation matrix must only consist of
        ///    translation.
        ///
        const BACKDROP_BLUR = 1 << 1;
    }
}

impl Default for LayerFlags {
    fn default() -> Self {
        Self::empty()
    }
}

/// Window event handlers.
///
/// The receiver is immutable because event handlers may manipulate windows,
/// which in turn might cause other event handlers to be called.
pub trait WndListener<T: Wm> {
    /// The user has attempted to close a window.
    fn close_requested(&self, _: T, _: &T::HWnd) {}

    /// The window got or lost focus.
    fn focus(&self, _: T, _: &T::HWnd) {}

    /// The window is ready to accept a new update.
    ///
    /// This method gets called after the client calls
    /// `Wm::request_update_ready_wnd`.
    ///
    /// The implementation may call `Wm::request_update_ready_wnd` for
    /// continuous animation.
    fn update_ready(&self, _: T, _: &T::HWnd) {}

    /// A window is being resized.
    ///
    /// While the user is resizing a window, this method is called repeatedly
    /// as the window's outline is dragged.
    ///
    /// The new window size can be retrieved using [`Wm::get_wnd_size`].
    /// Based on the new window size, The client (the implementer of this trait)
    /// should relayout, update composition layers, and call [`Wm::update_wnd`]
    /// in this method.
    fn resize(&self, _: T, _: &T::HWnd) {}

    /// The DPI scaling factor of a window has been updated.
    fn dpi_scale_changed(&self, _: T, _: &T::HWnd) {}

    /// The mouse pointer has moved inside a window when none of the mouse
    /// buttons are pressed (i.e., there is no active mouse drag gesture).
    fn mouse_motion(&self, _: T, _: &T::HWnd, _loc: Point2<f32>) {}

    /// The mouse pointer has left a window.
    fn mouse_leave(&self, _: T, _: &T::HWnd) {}

    /// Get event handlers for handling the mouse drag gesture initiated by
    /// a mouse down event described by `loc` and `button`.
    ///
    /// This method is called when a mouse button is pressed for the first time.
    /// The returned `MouseDragListener` will be used to handle mouse events
    /// (including the mouse down event that initiated the call) until all
    /// mouse buttons are released.
    fn mouse_drag(
        &self,
        _: T,
        _: &T::HWnd,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn MouseDragListener<T>> {
        Box::new(())
    }

    /// The mouse's scroll wheel was moved to scroll the contents underneath
    /// the mouse pointer.
    ///
    /// The system calls either `scroll_motion` or `scroll_gesture` to process
    /// scroll events. `scroll_motion` is used for an actual scroll wheel, while
    /// `scroll_gesture` is for a device such as a track pad that supports a
    /// continuous scroll operation.
    ///
    /// `scroll_motion` is never called when there is an active scroll gesture.
    fn scroll_motion(&self, _: T, _: &T::HWnd, _loc: Point2<f32>, _delta: &ScrollDelta) {}

    /// Get event handlers for handling the scroll gesture that started right
    /// now.
    fn scroll_gesture(&self, _: T, _: &T::HWnd, _loc: Point2<f32>) -> Box<dyn ScrollListener<T>> {
        Box::new(())
    }

    // TODO: more events
    //  - Pointer device gestures (swipe, zoom, rotate)
    //  - Keyboard
    //  - Input method
}

/// A default implementation of [`WndListener`].
impl<T: Wm> WndListener<T> for () {}

/// Mouse event handlers for mouse drag gestures.
///
/// A `MouseDragListener` object lives until one of the following events occur:
///
///  - `mouse_up` is called and there are no currently pressed buttons.
///  - `cancel` is called.
///
pub trait MouseDragListener<T: Wm> {
    /// The mouse pointer has moved inside a window when at least one of the
    /// mouse buttons are pressed.
    fn mouse_motion(&self, _: T, _: &T::HWnd, _loc: Point2<f32>) {}

    /// A mouse button was pressed inside a window.
    fn mouse_down(&self, _: T, _: &T::HWnd, _loc: Point2<f32>, _button: u8) {}

    /// A mouse button was released inside a window.
    ///
    /// When all mouse buttons are released, a reference to `MouseDragListener`
    /// is destroyed.
    /// A brand new `MouseDragListener` will be created via
    /// [`WndListener::mouse_drag`] next time a mouse button is pressed.
    ///
    /// [`WndListener::mouse_drag`]: crate::iface::WndListener::mouse_drag
    fn mouse_up(&self, _: T, _: &T::HWnd, _loc: Point2<f32>, _button: u8) {}

    /// A mouse drag gesture was cancelled.
    fn cancel(&self, _: T, _: &T::HWnd) {}
}

/// A default implementation of [`MouseDragListener`].
impl<T: Wm> MouseDragListener<T> for () {}

#[derive(Debug, Clone, Copy)]
pub struct ScrollDelta {
    /// The delta position. The meaning varies depending on `precise`.
    ///
    /// The signs of the components follow the movement of the scrolled contents.
    pub delta: Vector2<f32>,
    /// `true` if `delta` is measured in pixels. Otherwise, `delta` represents
    /// numbers of lines or rows.
    pub precise: bool,
}

impl Default for ScrollDelta {
    fn default() -> Self {
        Self {
            delta: Vector2::new(0.0, 0.0),
            precise: false,
        }
    }
}

/// Event handlers for scroll gestures.
///
/// A `ScrollListener` object lives until one of the following events occur:
///
///  - `end` is called.
///  - `cancel` is called.
///
pub trait ScrollListener<T: Wm> {
    /// The mouse's scroll wheel was moved.
    ///
    /// `velocity` represents the estimated current scroll speed, which is
    /// useful for implementing the rubber-band effect during intertia scrolling.
    fn motion(&self, _: T, _: &T::HWnd, _delta: &ScrollDelta, _velocity: Vector2<f32>) {}

    /// Mark the start of a momentum phase (also known as *inertia scrolling*).
    ///
    /// After calling this method, the system will keep generating `motion`
    /// events with dissipating delta values.
    fn start_momentum_phase(&self, _: T, _: &T::HWnd) {}

    /// The gesture was completed.
    fn end(&self, _: T, _: &T::HWnd) {}

    /// The gesture was cancelled.
    fn cancel(&self, _: T, _: &T::HWnd) {}
}

/// A default implementation of [`ScrollListener`].
impl<T: Wm> ScrollListener<T> for () {}

/// Describes the appearance of the mouse cursor.
///
/// This type contains the same set of variants as `winit::window::CursorIcon`
/// to allow cost-free conversion between these two types.
///
/// TODO: There is no point in copying `winit::window::CursorIcon` anymore.
///       Remove unused variants to reduce the code size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorShape {
    Default,
    Crosshair,
    Hand,
    Arrow,
    Move,
    Text,
    Wait,
    Help,
    Progress,
    NotAllowed,
    ContextMenu,
    Cell,
    VerticalText,
    Alias,
    Copy,
    NoDrop,
    Grab,
    Grabbing,
    AllScroll,
    ZoomIn,
    ZoomOut,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
}

impl Default for CursorShape {
    fn default() -> Self {
        CursorShape::Default
    }
}

/// Text context event handlers.
///
/// The receiver is immutable because event handlers may manipulate windows,
/// which in turn might cause other event handlers to be called.
///
/// The implementations of these methods are not allowed to use [`Wm`]'s methods
/// to manipulate the current text input context.
pub trait TextInputCtxListener<T: Wm> {
    /// Acquire a lock on the text document.
    ///
    /// Acquires a write lock if `mutating` is `true`. Some methods of
    /// `TextInputCtxEdit` requires a write lock. The behavior is unspecified
    /// when such methods are called without a write lock.
    ///
    /// This method is called from the top level of a main event loop and is
    /// expected to be able to acquire a lock successfully. In addition to this,
    /// there are some methods of [`Wm`] that may call this method, which are
    /// documented separately.
    fn edit(
        &self,
        wm: T,
        _: &T::HTextInputCtx,
        mutating: bool,
    ) -> Box<dyn TextInputCtxEdit<T> + '_>;

    /// Indicate the set of events recognized by the system.
    ///
    /// The client has to call one of [`Wm`]'s methods when making changes to
    /// the underlying contents of a text input context, but depending on
    /// various factors, some events don't have to be generated by the client.
    /// The client don't have to call the methods if they are not in `flags`.
    /// It can still do this, but the system will ignore the event and may have
    /// a negative performance ramification.
    fn set_event_mask(&self, _wm: T, _: &T::HTextInputCtx, _flags: TextInputCtxEventFlags) {}
}

bitflags! {
    pub struct TextInputCtxEventFlags: u8 {
        /// The system handles [`Wm::text_input_ctx_reset`].
        const RESET = 1;
        /// The system handles [`Wm::text_input_ctx_on_selection_change`].
        const SELECTION_CHANGE = 1 << 1;
        /// The system handles [`Wm::text_input_ctx_on_layout_change`].
        const LAYOUT_CHANGE = 1 << 2;
    }
}

/// Trait for objects representing a lock acquired by
/// [`TextInputCtxListener::edit`].
///
/// All given ranges are subsets of `0..self.len()`. All endpoints are on UTF-8
/// character boundaries. The client can use `TextInputCtxEdit::floor_index` to
/// find the nearest boundary.
///
/// The implementations of these methods are not allowed to use [`Wm`]'s methods
/// to manipulate the current text input context.
pub trait TextInputCtxEdit<T: Wm> {
    /// Get the current selection.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::GetSelection`,
    /// `[NSTextInputClient selectedRange]`
    ///
    /// `start` and `end` can be in any order.
    fn selected_range(&mut self) -> Range<usize>;

    /// Set the current selection.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::SetSelection`
    ///
    /// Requires a write lock.
    fn set_selected_range(&mut self, range: Range<usize>);

    /// Specify the portion of the text document being composed.
    ///
    /// Requires a write lock.
    ///
    /// This method roughly corresponds to:
    /// `ITfContextOwnerCompositionSink::OnStartComposition`,
    /// `ITfContextOwnerCompositionSink::OnUpdateComposition`,
    /// `[NSTextInputClient setMarkedText:selectedRange:replacementRange:]`
    /// (when `range` is `Some(_)`);
    /// `ITfContextOwnerCompositionSink::OnEndComposition`,
    /// `[NSTextInputClient unmarkText]`
    /// (when `range` is `None`)
    fn set_composition_range(&mut self, range: Option<Range<usize>>);

    // TODO: Support conditionally denying substitution
    /// Replace a portion of the text document.
    ///
    /// Requires a write lock.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::SetText`,
    /// `ITextStoreACP::InsertTextAtSelection`,
    /// `[NSTextInputClient insertText:replacementRange:]`
    fn replace(&mut self, range: Range<usize>, text: &str);

    /// Read a portion of the text document.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::GetText`,
    /// `[NSTextInputClient attributedSubstringForProposedRange:actualRange:]`
    fn slice(&mut self, range: Range<usize>) -> String;

    /// Round `i` to the previous UTF-8 character boundary.
    ///
    /// The returned index must be equal to or less than `i`.
    fn floor_index(&mut self, i: usize) -> usize;

    /// Round `i` to the next UTF-8 character boundary.
    ///
    /// The returned index must be equal to or greater than `i`, and must be
    /// equal to or less than `len()`.
    fn ceil_index(&mut self, i: usize) -> usize;

    /// Get the length of the text document.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::GetEndACP`
    fn len(&mut self) -> usize;

    /// Convert a point in the window coordinates to a UTF-8 offset.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::GetACPFromPoint`,
    /// `[NSTextInputClient characterIndexForPoint:]`
    fn index_from_point(&mut self, point: Point2<f32>, flags: IndexFromPointFlags)
        -> Option<usize>;

    /// Get the bounding rectangle in the window coordinates of the region where
    /// the text document is rendered.
    ///
    /// An invalid `Box2` (`x.is_valid() == false`) indicates the text document
    /// is currently invisible.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::GetScreenExt`
    fn frame(&mut self) -> Box2<f32>;

    /// Get the first logical bounding rectangle for a portion of the text
    /// document.
    ///
    /// Returns `(bounds, i)`. An invalid `Box2` (`bounds.is_valid() == false`)
    /// means the text document is currently invisible. `i` indicates the end
    /// index of the range enclosed by `bounds` and must be in range
    /// `range.start + 1 ..= range.end` if `range.start < range.end` or
    /// must be equal to `range.start` if `range.start == range.end`.
    ///
    /// This method roughly corresponds to: `ITextStoreACP::GetTextExt`,
    /// `[NSTextInputClient firstRectForCharacterRange:actualRange:]`
    fn slice_bounds(&mut self, range: Range<usize>) -> (Box2<f32>, usize);
}

bitflags! {
    pub struct IndexFromPointFlags: u8 {
        /// Requests that a nearest bounding edge instead of a character is to
        /// be found.
        const ROUND_NEAREST = 1;
        /// If the point is outside the bounding box of the target object, the
        /// closest position is returned.
        const CLAMP = 1 << 1;
    }
}

/// A immutable, ref-counted bitmap image.
pub trait Bitmap: Clone + Sized + Send + Sync + Debug {
    /// Get the dimensions of a bitmap.
    fn size(&self) -> [u32; 2];
}

/// Types supporting drawing operations.
pub trait Canvas: Debug {
    /// Push a copy of the current graphics state onto the state stack.
    fn save(&mut self);
    /// Pop a graphics state from the state stack.
    fn restore(&mut self);

    /// Start a new empty path.
    fn begin_path(&mut self);
    /// Close the current figure of the current path.
    fn close_path(&mut self);

    /// Begin a new subpath at the specified point.
    fn move_to(&mut self, p: Point2<f32>);
    /// Append a straight line to the specified point.
    fn line_to(&mut self, p: Point2<f32>);
    /// Append a cubic Bézier curve to the specified point, using the provided
    /// control points.
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>);
    /// Append a quadratic Bézier curve to the specified point, using the
    /// provided control point.
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>);

    /// Add a rectangle to the current path.
    fn rect(&mut self, bx: Box2<f32>) {
        super::canvas::canvas_rect(self, bx)
    }
    /// Add a rounded rectangle to the current path.
    ///
    /// `radii` specifies the corner radii (width/height) of the four corners of
    /// the rectangle in a clock-wise order, starting from the upper-left corner.
    /// Overlapping corner curves are handled based on [CSS's definition] - all
    /// corners are uniformly scaled down until no corner curves overlap.
    ///
    /// [CSS's definition]: https://drafts.csswg.org/css-backgrounds-3/#corner-overlap
    ///
    /// The behaviour with an invalid `bx` (having a negative width/height) is
    /// unspecified.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cggeom::box2;
    /// # use tcw3_pal::iface::Canvas;
    /// # fn test(canvas: &mut impl Canvas) {
    /// let bx = box2! { min: [0.0, 0.0], max: [50.0, 40.0] };
    /// // Rounded rectangle with a uniform radius
    /// canvas.rounded_rect(bx, [[5.0; 2]; 4]);
    /// // Rounded rectangle having four circular arcs with different radii
    /// canvas.rounded_rect(bx, [[1.0; 2], [2.0; 2], [3.0; 2], [4.0; 2]]);
    /// // Ellipse (no straight edges)
    /// canvas.rounded_rect(bx, [[25.0, 20.0]; 4]);
    /// # }
    /// ```
    fn rounded_rect(&mut self, bx: Box2<f32>, radii: [[f32; 2]; 4]) {
        super::canvas::canvas_rounded_rect(self, bx, radii)
    }
    /// Add an ellipse bounded by the specified rectangle to the current path.
    fn ellipse(&mut self, bx: Box2<f32>) {
        super::canvas::canvas_ellipse(self, bx)
    }

    /// Fill the area within the current path, using the non-zero winding number
    /// rule.
    fn fill(&mut self);
    /// Draw a line along the current path.
    fn stroke(&mut self);
    /// Set the current clipping region to its intersection with the area within
    /// current path.
    fn clip(&mut self);

    /// Stroke the specified rectangle.
    ///
    /// The implementation of this method may invalidate the current path.
    fn stroke_rect(&mut self, bx: Box2<f32>) {
        self.begin_path();
        self.rect(bx);
        self.stroke();
    }
    /// Fill the specified rectangle.
    ///
    /// The implementation of this method may invalidate the current path.
    fn fill_rect(&mut self, bx: Box2<f32>) {
        self.begin_path();
        self.rect(bx);
        self.fill();
    }
    /// Set the current clipping region to its intersection with the specified
    /// rectangle.
    ///
    /// The implementation of this method may invalidate the current path.
    fn clip_rect(&mut self, bx: Box2<f32>) {
        self.begin_path();
        self.rect(bx);
        self.clip();
    }

    /// Set the current fill brush to a solid color.
    fn set_fill_rgb(&mut self, rgb: RGBAF32);
    // TODO: generic brush

    /// Set the current stroke brush to a solid color.
    fn set_stroke_rgb(&mut self, rgb: RGBAF32);
    // TODO: generic brush

    fn set_line_cap(&mut self, cap: LineCap);
    fn set_line_join(&mut self, join: LineJoin);
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]);
    /// Set the line width in pixels. Defaults to `1.0`.
    ///
    /// Note that strokes are converted to a path before the current
    /// transformation matrix is applied. This means that, the rendered line
    /// width varies depending on the scaling factor of the CTM.
    fn set_line_width(&mut self, width: f32);
    fn set_line_miter_limit(&mut self, miter_limit: f32);

    /// Transform the local coordinate system.
    ///
    /// `m.x.z` and `m.y.z` is assumed to be zero. This means projective
    /// transformations are not supported and only affine transformations can
    /// be expressed. `m.z.z` must be positive.
    fn mult_transform(&mut self, m: Matrix3<f32>);

    // TODO: text rendering

    // TODO: image rendering
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
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

/// Encapsulates information needed to layout a given text.
///
/// This corresponds to `CTFrame` of Core Text, `IDWriteTextLayout` of
/// DirectWrite, and `PangoLayout` of Pango.
///
/// # Notes on the Complexity of the Methods
///
/// Some methods of this trait have time complexity requirements. Some of them
/// may require additional pre-processing steps, whose execution times are,
/// however, not accounted in the requirements.
pub trait TextLayout: Send + Sync + Sized {
    type CharStyle: CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self;
    // TODO: construct a `TextLayout` from an attributed text

    /// Get the visual bounds of a `TextLayout`.
    fn visual_bounds(&self) -> Box2<f32>;
    /// Get the layout bounds of a `TextLayout`.
    fn layout_bounds(&self) -> Box2<f32>;

    /// Find the character boundary (as a UTF-8 offset) closest to the given
    /// point.
    fn cursor_index_from_point(&self, point: Point2<f32>) -> usize;

    /// Determine the location of the cursor at the given UTF-8 offset.
    ///
    /// Two locations will be returned. One `Beam` represents the strong
    /// cursor location where characters of the directionality matcing the base
    /// writing direction are inserted. The other `Beam` represents the weak
    /// cursor location wher other characters are inserted. The order in which
    /// these two `Beam`s are returned is unspecified.
    ///
    /// `i` must be in range `0..=len` where `len` is the length of the source
    /// string.
    ///
    /// If `i` refers to a position inside a grapheme cluster, `i` will be
    /// rounded to a nearest boundary in an unspecified way.
    ///
    /// # Rationale
    ///
    /// Originally, the order of the two `Beam` was specified, but later it was
    /// found that `CTLineGetOffsetForStringIndex` may return two offsets in a
    /// different order.
    fn cursor_pos(&self, i: usize) -> [Beam; 2];

    /// Get the number of lines in the layout.
    fn num_lines(&self) -> usize;

    /// Get the UTF-8 offset range for the line `i`.
    ///
    /// `i` must be in range `0..num_lines()`.
    ///
    /// The ranges returned by this method are a partition of the source string.
    /// Each range includes any trailing line break character(s).
    ///
    /// # Complexity
    ///
    /// The time complexity of this method is `O(1)`.
    fn line_index_range(&self, i: usize) -> Range<usize>;

    /// Get the line containing the given UTF-8 offset.
    ///
    /// `i` must be in range `0..=len` where `len` is the length of the source
    /// string. If `i == len`, this function returns `self.num_lines() - 1`.
    ///
    /// # Complexity
    ///
    /// The time complexity of this method is `O(log(num_lines))`.
    fn line_from_index(&self, i: usize) -> usize {
        let mut base = 0;
        let mut size = self.num_lines();
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            base = if i >= self.line_index_range(mid).start {
                mid
            } else {
                base
            };
            size -= half;
        }
        base
    }

    /// Get the vertical geometric range for the line `i` that can be used for
    /// displaying selection and hit testing.
    ///
    /// `i` must be in range `0..num_lines()`.
    ///
    /// The ranges returned by this method are a partition of `layout_bounds()`.
    ///
    /// # Complexity
    ///
    /// The time complexity of this method is `O(1)`.
    fn line_vertical_bounds(&self, i: usize) -> Range<f32>;

    /// Get the Y coordinate of the baseline of the line `i` that can be used
    /// for positioning the text.
    ///
    /// `i` must be in range `0..num_lines()`.
    ///
    /// # Complexity
    ///
    /// The time complexity of this method is `O(1)`.
    fn line_baseline(&self, i: usize) -> f32;

    /// Get a list of `RunMetrics` for a UTF-8 offset range. The returned
    /// elements are stored in the visual (top to bottom, left to right) order.
    ///
    /// `i` must be an improper subset of `0..num_lines()`. `i.end` must be
    /// greater than `i.start`. `i` must not span across multiple lines; i.e.,
    /// there must be some `range = self.line_index_range(line)` for which
    /// `range.contains(i.start) && range.contains(i.end - 1)`.
    /// (This means the return value of `line_index_range` is a valid range to
    /// pass to `run_metrics_of_range`.)
    ///
    /// The set of the values of `RunMetrics::index` returned by this method are
    /// a partition of `i`. This means trailing newline chracters are included
    /// in the result provided that the given range includes them.
    ///
    /// If some of the specified endpoints refer to positions inside grapheme
    /// clusters, they will be rounded to nearest boundaries in an unspecified
    /// way.
    ///
    /// # Complexity
    ///
    /// The time complexity of this method is
    /// `O(line_len_8*log(line_len_8) + line_len_16*log(line_len_16) + line_i)`
    /// where `line_len_8` and `line_len_16` are the lengths of the *whole* line
    /// encoded in UTF-8 and UTF-16, respectively, and `line_i` is the number of
    /// the lines before the one containing `i`.
    fn run_metrics_of_range(&self, i: Range<usize>) -> Vec<RunMetrics>;

    // TODO: alignment
    // TODO: inline/foreign object
}

/// Represents the geometric position of an insertion cursor within a text
/// layout. This is essentially a `Box2<f32>` with a zero width.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct Beam {
    pub x: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Beam {
    /// Construct a `Beam` with given corner coordinates.
    pub fn new(x: f32, top: f32, bottom: f32) -> Self {
        Self { x, top, bottom }
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// Convert this `Beam` to `Box2`.
    #[inline]
    pub fn as_box2(&self) -> Box2<f32> {
        box2! {
            min: [self.x, self.top],
            max: [self.x, self.bottom],
        }
    }

    /// Convert this `Beam` to `Box2` with a given width.
    #[inline]
    pub fn as_wide_box2(&self, width: f32) -> Box2<f32> {
        box2! {
            min: [self.x - width / 2.0, self.top],
            max: [self.x + width / 2.0, self.bottom],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunMetrics {
    pub flags: RunFlags,
    /// A UTF-8 offset range in the source text.
    pub index: Range<usize>,
    /// The bounding box that can be used for displaying selection and
    /// hit testing.
    pub bounds: Range<f32>,
}

bitflags! {
    pub struct RunFlags: u8 {
        /// The run proceeds from right to left.
        ///
        /// # Rationale
        ///
        /// This flag corresponds to `kCTRunStatusRightToLeft` from Core Text.
        /// `CTRun` from Core Text doesn't have a BiDi level, which other APIs
        /// have as exemplified by `DWRITE_HIT_TEST_METRICS::bidiLevel`
        /// (DirectWrite) and `PangoAnalysis::level` (Pango).
        const RIGHT_TO_LEFT = 1;
    }
}

impl Default for RunFlags {
    fn default() -> Self {
        RunFlags::empty()
    }
}

pub trait CanvasText<TLayout>: Canvas {
    fn draw_text(&mut self, layout: &TLayout, origin: Point2<f32>, color: RGBAF32);
}

/// An immutable, thread-safe handle type representing a character style.
pub trait CharStyle: Clone + Send + Sync + Sized {
    /// Construct a `CharStyle`.
    fn new(attrs: CharStyleAttrs<Self>) -> Self;

    /// Get the font size.
    fn size(&self) -> f32;
}

#[allow(clippy::option_option)] // for consistency between fields
#[derive(Debug, Clone)]
pub struct CharStyleAttrs<TCharStyle> {
    pub template: Option<TCharStyle>,
    pub sys: Option<SysFontType>,
    pub size: Option<f32>,
    pub decor: Option<TextDecorFlags>,
    /// The text color.
    ///
    /// The color value passed to [`CanvasText::draw_text`] is used if `None` is
    /// specified.
    pub color: Option<Option<RGBAF32>>,
}

impl<TCharStyle> Default for CharStyleAttrs<TCharStyle> {
    fn default() -> Self {
        Self {
            template: None,
            sys: None,
            size: None,
            decor: None,
            color: None,
        }
    }
}

bitflags! {
    pub struct TextDecorFlags: u8 {
        const UNDERLINE = 1;
        const OVERLINE = 1 << 1;
        const STRIKETHROUGH = 1 << 2;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SysFontType {
    /// The font used for UI elements.
    Normal,
    /// The font used for emphasis in UI elements.
    Emph,
    /// The font used for small UI elements.
    Small,
    /// The font used for emphasis in small UI elements.
    SmallEmph,
    /// The font used for editable document.
    User,
    /// The monospace font used for editable document.
    UserMonospace,
}
