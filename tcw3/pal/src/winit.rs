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
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy, EventLoopWindowTarget};

use super::{iface::WM, MtSticky};

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

impl<TWM: WM> WinitEnv<TWM> {
    pub const fn new() -> Self {
        Self {
            mt: OnceCell::new(),
            pending_invoke_events: OnceCell::new(),
        }
    }

    /// Check if the calling thread is the main thread. If no thread is
    /// marked as the main thread yet, *mark the current thread as one*,
    /// returning `true`.
    ///
    /// `TWM` should use this method to implement `WM::is_main_thread`. This
    /// is the canonical source of a predicate defining what is the main
    /// thread and what is not.
    #[inline]
    pub fn is_main_thread(&self) -> bool {
        self.mt_data_or_init().mt_check.try_get().is_ok()
    }

    fn mt_data_or_init(&self) -> &MtData<TWM> {
        self.mt.get().unwrap_or_else(|| self.mt_data_or_init_slow())
    }

    #[cold]
    fn mt_data_or_init_slow(&self) -> &MtData<TWM> {
        let mut lock = None;

        self.mt.get_or_init(|| {
            // Mark the current thread as the main thread
            let mt_check = Fragile::new(());

            // *We* define the current thread as the main thread, so this
            // should be safe
            let wm = unsafe { TWM::global_unchecked() };

            // Create a winit event loop
            let mut winit_wm = WinitWm::new(wm);
            let proxy = winit_wm.create_proxy();

            // Acquire a lock on `pending_invoke_events` to process pending
            // events.
            let mut pending_invoke_events = self
                .pending_invoke_events
                .get_or_init(Default::default)
                .lock()
                .unwrap();

            Self::handle_pending_invoke_events(&mut pending_invoke_events, &proxy);

            // The lock must survive until `self.mt` is initialized. Otherwise,
            // we might miss some events, which would be stuck in
            // `pending_invoke_events` that we would never check again.
            lock = Some(pending_invoke_events);

            MtData {
                mt_check,
                // *We* define the current thread as the main thread, so this
                // should be safe
                wm: MtSticky::with_wm(wm, winit_wm),
                proxy,
            }
        })
    }

    #[inline]
    pub fn wm_with_wm(&'static self, wm: TWM) -> &WinitWm<TWM> {
        self.mt_data_or_init().wm.get_with_wm(wm)
    }

    pub fn invoke_on_main_thread(
        &'static self,
        cb: impl FnOnce(&'static WinitWm<TWM>) + Send + 'static,
    ) {
        let e: UserEvent<TWM> = Box::new(cb);

        if let Some(mt) = self.mt.get() {
            let _ = mt.proxy.send_event(e);
            return;
        }

        self.invoke_on_main_thread_slow(e)
    }

    #[cold]
    fn invoke_on_main_thread_slow(&self, e: UserEvent<TWM>) {
        // `EventLoop` might not be there yet, so push the event to
        // the ephemeral queue we manage
        let mut pending_invoke_events = self
            .pending_invoke_events
            .get_or_init(Default::default)
            .lock()
            .unwrap();
        pending_invoke_events.push(e);

        // Check `mt` again. It might have been initialized while we were
        // updating `pending_invoke_events`.
        if let Some(mt) = self.mt.get() {
            Self::handle_pending_invoke_events(&mut pending_invoke_events, &mt.proxy);
        }
    }

    #[cold]
    fn handle_pending_invoke_events(
        pending_invoke_events: &mut Vec<UserEvent<TWM>>,
        proxy: &EventLoopProxy<UserEvent<TWM>>,
    ) {
        for e in std::mem::replace(pending_invoke_events, Vec::new()) {
            // Ignore `EventLoopClosed`
            let _ = proxy.send_event(e);
        }
    }
}

impl<TWM: WM> WinitWm<TWM> {
    pub fn new(wm: TWM) -> Self {
        Self {
            wm,
            event_loop: RefCell::new(Some(EventLoop::new_user_event())),
            should_terminate: Cell::new(false),
            event_loop_wnd_target: Cell::new(None),
            unsend_invoke_events: RefCell::new(LinkedList::new()),
        }
    }

    pub fn wm(&self) -> TWM {
        self.wm
    }

    fn create_proxy(&mut self) -> EventLoopProxy<UserEvent<TWM>> {
        self.event_loop.get_mut().as_ref().unwrap().create_proxy()
    }

    pub fn enter_main_loop(&'static self) -> ! {
        let event_loop = self
            .event_loop
            .replace(None)
            .expect("can't call enter_main_loop twice");

        struct Guard<'a, TWM: WM>(&'a Cell<Option<NonNull<EventLoopWindowTarget<UserEvent<TWM>>>>>);

        impl<TWM: WM> Drop for Guard<'_, TWM> {
            fn drop(&mut self) {
                self.0.set(None);
            }
        }

        event_loop.run(move |event, event_loop_wnd_target, control_flow| {
            // Allow the inner code to access `event_loop_wnd_target`. Make sure
            // to remove it from `self.event_loop_wnd_target` after the function
            // call (hence the guard).
            self.event_loop_wnd_target
                .set(Some(NonNull::from(event_loop_wnd_target)));
            let _guard = Guard(&self.event_loop_wnd_target);

            match event {
                _ => {}
            }
            // TODO

            while let Some(e) = self.unsend_invoke_events.borrow_mut().pop_front() {
                e(self);
            }

            if self.should_terminate.get() {
                *control_flow = ControlFlow::Exit;
            } else {
                *control_flow = ControlFlow::Wait;
            }
        });
    }

    pub fn terminate(&self) {
        self.should_terminate.set(true);
    }

    /// Call a function using the `EventLoopWindowTarget` supplied by `EventLoop`
    /// or something.
    ///
    /// This possibly immutable borrows `EventLoop`, thus the callback function
    /// must not call `enter_main_loop`.
    fn with_event_loop_wnd_target<R>(
        &self,
        f: impl FnOnce(&EventLoopWindowTarget<UserEvent<TWM>>) -> R,
    ) -> R {
        let target;
        let borrow;

        let maybe_ptr = self.event_loop_wnd_target.get();
        if let Some(ref ptr) = maybe_ptr {
            // We are inside the main event loop (executed by `enter_main_loop`).
            // In this case, `EventLoop` already has been moved out. However,
            // a reference to `EventLoopWindowTarget` is instead available
            // through a cell.
            //
            // The reference is invalidated after each iteration of the event
            // loop, hence the callback style of `with_event_loop_wnd_target`.
            // This `unsafe` is completely safe because `target` will never
            // outlive this function's scope, which is entirely contained by
            // the actual lifetime of `target`'s referent.
            target = unsafe { ptr.as_ref() };
        } else {
            // The main event loop hasn't started yet, thus `EventLoop` is
            // still accessible. `EventLoop` derefs to `EventLoopWindowTarget`.
            borrow = self.event_loop.borrow();
            target = &borrow.as_ref().unwrap();
        }

        f(target)
    }

    pub fn invoke(&'static self, cb: impl FnOnce(&'static WinitWm<TWM>) + 'static) {
        self.unsend_invoke_events
            .borrow_mut()
            .push_back(Box::new(cb));
    }
}
