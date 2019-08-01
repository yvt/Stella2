//! The backend implementation based on [winit].
//!
//! [winit]: https://github.com/rust-windowing/winit
//!
//! winit only deals with window handling and does not concern with window
//! contents. A platform-specific module may delegate window handling to this
//! module, but should implement window content rendering by themselves by
//! invoking their respective platform APIs.
use fragile::Fragile;
use freeze::{FreezableCell, FreezableCellRef};
use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy, EventLoopWindowTarget};

use super::{iface::WM, MtSticky};

// TODO

/// The user event type.
type UserEvent = Box<dyn FnOnce(&'static WinitWm) + Send>;

pub struct WinitEnv<TWM: WM> {
    mt_check: FreezableCell<Option<Fragile<()>>>,
    wm_and_proxy: FreezableCell<Option<WmAndProxy<TWM>>>,
}

struct WmAndProxy<TWM: WM> {
    wm: MtSticky<WinitWm, TWM>,
    proxy: EventLoopProxy<UserEvent>,
}

pub struct WinitWm {
    /// This `EventLoop` is wrapped by `RefCell` so that it can be moved out when
    /// starting the main event loop.
    event_loop: RefCell<Option<EventLoop<UserEvent>>>,
    should_terminate: Cell<bool>,
    /// This is a handle used to create `winit::window::Window` from the inside
    /// of `run`. It's a reference supplied to the event handler function that
    /// only lives through a single iteration of the main event loop.
    event_loop_wnd_target: Cell<Option<NonNull<EventLoopWindowTarget<UserEvent>>>>,
}

impl<TWM: WM> WinitEnv<TWM> {
    pub const fn new() -> Self {
        Self {
            mt_check: FreezableCell::new_unfrozen(None),
            // It's safe to send `None` even if `Some(x)` isn't sendable
            wm_and_proxy: FreezableCell::new_unfrozen(None),
        }
    }

    /// Check if the calling thread is the main thread. If no thread is
    /// marked as the main thread yet, *mark the current thread as one*,
    /// returning `true`.
    ///
    /// Panics on a race condition (e.g., when multiple threads call this at
    /// the same time).
    ///
    /// Assuming `TWM` uses this method to implement `WM::is_main_thread`, this
    /// is the canonical source of a predicate defining what is the main
    /// thread and what is not.
    #[inline]
    pub fn is_main_thread(&self) -> bool {
        if let Ok(Some(fragile)) = self.mt_check.frozen_borrow() {
            // Some thread is already registered as the main thread. The
            // contained `Fragile` is bound to that thread.
            return fragile.try_get().is_ok();
        }

        // Mark the current thread as the main thread.
        self.mark_main_thread()
    }

    #[cold]
    fn mark_main_thread(&self) -> bool {
        let mut lock = self
            .mt_check
            .unfrozen_borrow_mut()
            .expect("race condition detected");

        debug_assert!(lock.is_none());
        *lock = Some(Fragile::new(()));

        FreezableCellRef::freeze(lock);

        // A return value of `is_main_thread`. By returning it here, we enable
        // the tail call optimization for `in_main_thread`.
        true
    }

    #[inline]
    pub fn wm_with_wm(&'static self, wm: TWM) -> &WinitWm {
        if let Ok(Some(wm_and_proxy)) = self.wm_and_proxy.frozen_borrow() {
            wm_and_proxy.wm.get_with_wm(wm)
        } else {
            self.wm_with_wm_slow(wm)
        }
    }

    #[cold]
    fn wm_with_wm_slow(&'static self, wm: TWM) -> &WinitWm {
        // This is not supposed to fail unless `WinitWm::new()` calls this
        // method recursively
        let mut lock = self.wm_and_proxy.unfrozen_borrow_mut().unwrap();

        debug_assert!(lock.is_none());

        let mut winit_wm = WinitWm::new();
        let proxy = winit_wm.create_proxy();
        *lock = Some(WmAndProxy {
            wm: MtSticky::with_wm(wm, winit_wm),
            proxy,
        });

        FreezableCellRef::freeze(lock)
            .as_ref()
            .unwrap()
            .wm
            .get_with_wm(wm)
    }

    pub fn invoke_on_main_thread(
        &'static self,
        cb: impl FnOnce(&'static WinitWm) + Send + 'static,
    ) {
        unimplemented!()
    }
}

impl WinitWm {
    pub fn new() -> Self {
        Self {
            event_loop: RefCell::new(Some(EventLoop::new_user_event())),
            should_terminate: Cell::new(false),
            event_loop_wnd_target: Cell::new(None),
        }
    }

    fn create_proxy(&mut self) -> EventLoopProxy<UserEvent> {
        self.event_loop.get_mut().as_ref().unwrap().create_proxy()
    }

    pub fn enter_main_loop(&'static self) -> ! {
        let event_loop = self
            .event_loop
            .replace(None)
            .expect("can't call enter_main_loop twice");

        struct Guard<'a>(&'a Cell<Option<NonNull<EventLoopWindowTarget<UserEvent>>>>);

        impl Drop for Guard<'_> {
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

            // TODO

            // TODO: Move `event_loop_wnd_target`

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
        f: impl FnOnce(&EventLoopWindowTarget<UserEvent>) -> R,
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

    pub fn invoke(&'static self, cb: impl FnOnce(&'static WinitWm) + 'static) {
        unimplemented!()
    }
}
