//! The backend implementation based on [winit].
//!
//! [winit]: https://github.com/rust-windowing/winit
//!
//! winit only deals with window handling and does not concern with window
//! contents. A platform-specific module may delegate window handling to this
//! module, but should implement window content rendering by themselves by
//! invoking their respective platform APIs.
//!
//! # Window Redraw Interface
//!
//! The possible pathways though which an update is done are summarized
//! in the following quasi-call graph, separated by the types of initiators:
//!
//! ```text
//! Wm::request_update_ready_wnd
//!  └─ Window::request_redraw
//!      └─ (event handler of) WindowEvent::RedrawRequested
//!          ├─ WndListener::update_ready
//!          │   └─ Wm::update_wnd
//!          │       └─ WndContent::update
//!          └─ WndContent::redraw_requested
//!
//! (event handler of) WindowEvent::Resized
//!  ├─ WndListener::resize
//!  │   └─ Wm::update_wnd
//!  │       └─ WndContent::update
//!  └─ WndContent::redraw_requested
//!
//! (event handler of) WindowEvent::RedrawRequested
//!  └─ WndContent::redraw_requested
//!
//! Wm::update_wnd
//!  ├─ WndContent::update
//!  └─ Window::request_redraw (if update returns true)
//!      └─ (event handler of) WindowEvent::RedrawRequested
//!          └─ WndContent::redraw_requested
//! ```
use cgmath::Point2;
use fragile::Fragile;
use iterpool::{Pool, PoolPtr};
use neo_linked_list::{AssertUnpin, LinkedListCell};
use once_cell::sync::OnceCell;
use std::{
    cell::{Cell, RefCell},
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
    timerqueue::{HTask, TimerQueue},
    MtSticky,
};

mod utils;
mod window;
mod wm;

/// The user event type.
type UserEvent<TWM, TWC> = Box<dyn FnOnce(&'static WinitWmCore<TWM, TWC>) + Send>;

type EventLoopWndTargetPtr<TWM, TWC> = NonNull<EventLoopWindowTarget<UserEvent<TWM, TWC>>>;

type UnsendInvoke<TWM, TWC> = dyn FnOnce(&'static WinitWmCore<TWM, TWC>);

type UnsendInvokeBox<TWM, TWC> = Box<UnsendInvoke<TWM, TWC>>;

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
    event_loop_wnd_target: Cell<Option<EventLoopWndTargetPtr<TWM, TWC>>>,
    unsend_invoke_events: LinkedListCell<AssertUnpin<UnsendInvoke<TWM, TWC>>>,
    /// This field must be unborrowed before entering user code.
    /// Perhaps, the runtime checks can be removed by type sorcery...?
    timer_queue: RefCell<TimerQueue<UnsendInvokeBox<TWM, TWC>>>,

    /// A list of open windows. To support reentrancy, this must be unborrowed
    /// before calling any user event handlers.
    wnds: RefCell<Pool<Rc<Wnd<TWM, TWC>>>>,

    suppress_request_redraw: Cell<bool>,
}

/// Represents a type wrapping `WinitWmCore` to implement `Wm`.
pub trait WinitWm: Wm {
    /// Convert `HWndCore` to a backend-specific `HWnd`. Panic if the given window
    /// handle is invalid.
    fn hwnd_core_to_hwnd(self, hwnd: &HWndCore) -> Self::HWnd;

    /// Called once after `WinitWmCore` is created.
    fn init(self) {}
}

/// The invocation handle type used by `WinitWmCore`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvokeCore {
    htask: HTask,
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
    ///
    /// Returns `true` if `winit_wnd::request_redraw` needs to be called (with
    /// some conditions).
    /// The implementation should return `false` if the window system provides
    /// retained-mode rendering and the `RedrawRequested` event does not need to
    /// be handled.
    fn update(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) -> bool {
        true
    }

    /// Called as a response to the `RedrawRequested` event or in a `Resize`
    /// event handler.
    ///
    /// This method must be implemented if the window system does not retain
    /// window contents.
    fn redraw_requested(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) {}

    fn close(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) {}
}

struct Wnd<TWM: Wm, TWC> {
    winit_wnd: Window,
    content: RefCell<TWC>,
    listener: RefCell<Box<dyn WndListener<TWM>>>,
    mouse_drag: RefCell<Option<WndMouseDrag<TWM>>>,
    mouse_pos: Cell<Point2<f32>>,
    waiting_update_ready: Cell<bool>,
}

struct WndMouseDrag<TWM: Wm> {
    listener: Box<dyn MouseDragListener<TWM>>,
    /// A bit set of mouse buttons which are currently pressed down.
    pressed_buttons: u64,
}
