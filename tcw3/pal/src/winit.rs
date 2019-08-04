//! The backend implementation based on [winit].
//!
//! [winit]: https://github.com/rust-windowing/winit
//!
//! winit only deals with window handling and does not concern with window
//! contents. A platform-specific module may delegate window handling to this
//! module, but should implement window content rendering by themselves by
//! invoking their respective platform APIs.
use fragile::Fragile;
use once_cell::sync::OnceCell;
use std::{
    cell::{Cell, RefCell},
    collections::LinkedList,
    ptr::NonNull,
    sync::Mutex,
};
use winit::event_loop::{EventLoop, EventLoopProxy, EventLoopWindowTarget};

use super::{iface::WM, MtSticky};

mod wm;

/// The user event type.
type UserEvent<TWM> = Box<dyn FnOnce(&'static WinitWm<TWM>) + Send>;

pub struct WinitEnv<TWM: WM> {
    mt: OnceCell<MtData<TWM>>,
    /// Invoke events which were created before `mt` is initialized.
    pending_invoke_events: OnceCell<Mutex<Vec<UserEvent<TWM>>>>,
}

/// Things bound to the main thread.
struct MtData<TWM: WM> {
    /// `Fragile`'s content is only accessible to the initializing thread. We
    /// leverage this property to implement `is_main_thread`.
    mt_check: Fragile<()>,
    wm: MtSticky<WinitWm<TWM>, TWM>,
    proxy: EventLoopProxy<UserEvent<TWM>>,
}

pub struct WinitWm<TWM: WM> {
    wm: TWM,
    /// This `EventLoop` is wrapped by `RefCell` so that it can be moved out when
    /// starting the main event loop.
    event_loop: RefCell<Option<EventLoop<UserEvent<TWM>>>>,
    should_terminate: Cell<bool>,
    /// This is a handle used to create `winit::window::Window` from the inside
    /// of `run`. It's a reference supplied to the event handler function that
    /// only lives through a single iteration of the main event loop.
    event_loop_wnd_target: Cell<Option<NonNull<EventLoopWindowTarget<UserEvent<TWM>>>>>,
    unsend_invoke_events: RefCell<LinkedList<Box<dyn FnOnce(&'static WinitWm<TWM>)>>>,
}
