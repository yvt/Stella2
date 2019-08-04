//! The backend implementation based on [winit].
//!
//! [winit]: https://github.com/rust-windowing/winit
//!
//! winit only deals with window handling and does not concern with window
//! contents. A platform-specific module may delegate window handling to this
//! module, but should implement window content rendering by themselves by
//! invoking their respective platform APIs.
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

use super::{iface::WM, MtSticky};

mod wm;

/// The user event type.
type UserEvent<TWM, TWC> = Box<dyn FnOnce(&'static WinitWm<TWM, TWC>) + Send>;

/// The global state of the window manager, accessible by any threads.
/// `WinitWm` is included in this struct, protected by `MtSticky`. This struct
/// is also responsible for defining what is the main thread and what is not.
pub struct WinitEnv<TWM: WM, TWC: WndContent> {
    mt: OnceCell<MtData<TWM, TWC>>,
    /// Invoke events which were created before `mt` is initialized.
    pending_invoke_events: OnceCell<Mutex<Vec<UserEvent<TWM, TWC>>>>,
}

/// Things bound to the main thread.
struct MtData<TWM: WM, TWC: WndContent> {
    /// `Fragile`'s content is only accessible to the initializing thread. We
    /// leverage this property to implement `is_main_thread`.
    mt_check: Fragile<()>,
    wm: MtSticky<WinitWm<TWM, TWC>, TWM>,
    proxy: EventLoopProxy<UserEvent<TWM, TWC>>,
}

/// The global state of the window manager, only accessible to the main thread.
pub struct WinitWm<TWM: WM, TWC: WndContent> {
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
    wnds: RefCell<Pool<Rc<Wnd<TWC>>>>,
}

#[derive(Debug, Clone)]
pub struct HWnd {
    ptr: PoolPtr,
}

pub trait WndContent: 'static {}

struct Wnd<TWC> {
    winit_wnd: Window,
    content: TWC,
}
