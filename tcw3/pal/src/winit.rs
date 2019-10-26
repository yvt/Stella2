//! The backend implementation based on [winit].
//!
//! [winit]: https://github.com/rust-windowing/winit
//!
//! winit only deals with window handling and does not concern with window
//! contents. A platform-specific module may delegate window handling to this
//! module, but should implement window content rendering by themselves by
//! invoking their respective platform APIs.
use cgmath::Point2;
use fragile::Fragile;
use iterpool::{Pool, PoolPtr};
use once_cell::sync::OnceCell;
use std::{
    cell::{Cell, RefCell},
    collections::LinkedList,
    ptr::NonNull,
    rc::Rc,
    sync::Mutex,
};
use winit::{
    event_loop::{EventLoop, EventLoopProxy, EventLoopWindowTarget},
    window::Window,
};

use super::{
    iface::{MouseDragListener, Wm, WndListener},
    MtSticky,
};

mod utils;
mod window;
mod wm;

/// The user event type.
type UserEvent<TWM, TWC> = Box<dyn FnOnce(&'static WinitWmCore<TWM, TWC>) + Send>;

/// The global state of the window manager, accessible by any threads.
/// `WinitWmCore` is included in this struct, protected by `MtSticky`. This struct
/// is also responsible for defining what is the main thread and what is not.
pub struct WinitEnv<TWM: Wm, TWC: WndContent> {
    mt: OnceCell<MtData<TWM, TWC>>,
    /// Invoke events which were created before `mt` is initialized.
    pending_invoke_events: OnceCell<Mutex<Vec<UserEvent<TWM, TWC>>>>,
}

/// Things bound to the main thread.
struct MtData<TWM: Wm, TWC: WndContent> {
    /// `Fragile`'s content is only accessible to the initializing thread. We
    /// leverage this property to implement `is_main_thread`.
    mt_check: Fragile<()>,
    wm: MtSticky<WinitWmCore<TWM, TWC>, TWM>,
    proxy: Mutex<EventLoopProxy<UserEvent<TWM, TWC>>>,
}

/// The global state of the window manager, only accessible to the main thread.
pub struct WinitWmCore<TWM: Wm, TWC: WndContent> {
    wm: TWM,
    /// This `EventLoop` is wrapped by `RefCell` so that it can be moved out when
    /// starting the main event loop.
    event_loop: RefCell<Option<EventLoop<UserEvent<TWM, TWC>>>>,
    should_terminate: Cell<bool>,
    /// This is a handle used to create `winit::window::Window` from the inside
    /// of `run`. It's a reference supplied to the event handler function that
    /// only lives through a single iteration of the main event loop.
    event_loop_wnd_target: Cell<Option<NonNull<EventLoopWindowTarget<UserEvent<TWM, TWC>>>>>,
    unsend_invoke_events: RefCell<LinkedList<Box<dyn FnOnce(&'static Self)>>>,

    /// A list of open windows. To support reentrancy, this must be unborrowed
    /// before calling any user event handlers.
    wnds: RefCell<Pool<Rc<Wnd<TWM, TWC>>>>,
}

/// Represents a type wrapping `WinitWmCore` to implement `Wm`.
pub trait WinitWm: Wm {
    /// Convert `HWndCore` to a backend-specific `HWnd`. Panic if the given window
    /// handle is invalid.
    fn hwnd_core_to_hwnd(self, hwnd: &HWndCore) -> Self::HWnd;

    /// Called once after `WinitWmCore` is created.
    fn init(self) {}
}

/// The window handle type used by `WinitWmCore`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWndCore {
    ptr: PoolPtr,
}

pub trait WndContent: 'static + Sized {
    /// A window manager type.
    type Wm: Wm;

    /// A layer handle type that this `WndContent` accepts as the root layer.
    type HLayer: std::fmt::Debug + Clone;

    /// Called when a new root layer is attached. Redraw can be deferred until
    /// the next call to `update` or `paint`.
    fn set_layer(
        &mut self,
        wm: &WinitWmCore<Self::Wm, Self>,
        winit_wnd: &Window,
        layer: Option<Self::HLayer>,
    );

    /// Called inside `update_wnd`.
    fn update(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) {}

    /// Called as a response to the `RedrawRequested` event. Note that `WinitWmCore`
    /// does not automatically call `request_redraw`.
    fn redraw_requested(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) {}

    fn close(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) {}
}

struct Wnd<TWM: Wm, TWC> {
    winit_wnd: Window,
    content: RefCell<TWC>,
    listener: RefCell<Box<dyn WndListener<TWM>>>,
    mouse_drag: RefCell<Option<WndMouseDrag<TWM>>>,
    mouse_pos: Cell<Point2<f32>>,
}

struct WndMouseDrag<TWM: Wm> {
    listener: Box<dyn MouseDragListener<TWM>>,
    /// A bit set of mouse buttons which are currently pressed down.
    pressed_buttons: u64,
}
