use arrayvec::ArrayVec;
use fragile::Fragile;
use iterpool::Pool;
use once_cell::sync::OnceCell;
use owning_ref::OwningRef;
use std::{
    cell::{Cell, RefCell},
    collections::LinkedList,
    ops::Range,
    ptr::NonNull,
    sync::Mutex,
    time::Duration,
};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy, EventLoopWindowTarget};

use super::super::{timerqueue::TimerQueue, MtSticky};
use super::{HInvokeCore, MtData, UserEvent, WinitEnv, WinitWm, WinitWmCore, WndContent};

impl<TWM: WinitWm, TWC: WndContent<Wm = TWM>> WinitEnv<TWM, TWC> {
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
    /// `TWM` should use this method to implement `Wm::is_main_thread`. This
    /// is the canonical source of a predicate defining what is the main
    /// thread and what is not.
    #[inline]
    pub fn is_main_thread(&self) -> bool {
        self.mt_data_or_init().mt_check.try_get().is_ok()
    }

    fn mt_data_or_init(&self) -> &MtData<TWM, TWC> {
        self.mt.get().unwrap_or_else(|| self.mt_data_or_init_slow())
    }

    #[cold]
    fn mt_data_or_init_slow(&self) -> &MtData<TWM, TWC> {
        let mut lock = None;
        let mut wm_cell = None;

        let mt_data = self.mt.get_or_init(|| {
            // Mark the current thread as the main thread
            let mt_check = Fragile::new(());

            // *We* define the current thread as the main thread, so this
            // should be safe
            let wm = unsafe { TWM::global_unchecked() };

            // Create a winit event loop
            let mut winit_wm = WinitWmCore::new(wm);
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

            wm_cell = Some(wm);

            MtData {
                mt_check,
                // *We* define the current thread as the main thread, so this
                // should be safe
                wm: MtSticky::with_wm(wm, winit_wm),
                proxy: Mutex::new(proxy),
            }
        });

        // Call the `init` hook if we just created an instance of `WinitWmCore`.
        if let Some(wm) = wm_cell {
            wm.init();
        }

        mt_data
    }

    #[inline]
    pub fn wm_with_wm(&'static self, wm: TWM) -> &WinitWmCore<TWM, TWC> {
        self.mt_data_or_init().wm.get_with_wm(wm)
    }

    pub fn invoke_on_main_thread(
        &'static self,
        cb: impl FnOnce(&'static WinitWmCore<TWM, TWC>) + Send + 'static,
    ) {
        let e: UserEvent<TWM, TWC> = Box::new(cb);

        if let Some(mt) = self.mt.get() {
            let _ = mt.proxy.lock().unwrap().send_event(e);
            return;
        }

        self.invoke_on_main_thread_slow(e)
    }

    #[cold]
    fn invoke_on_main_thread_slow(&self, e: UserEvent<TWM, TWC>) {
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
            Self::handle_pending_invoke_events(
                &mut pending_invoke_events,
                &mt.proxy.lock().unwrap(),
            );
        }
    }

    #[cold]
    fn handle_pending_invoke_events(
        pending_invoke_events: &mut Vec<UserEvent<TWM, TWC>>,
        proxy: &EventLoopProxy<UserEvent<TWM, TWC>>,
    ) {
        for e in std::mem::replace(pending_invoke_events, Vec::new()) {
            // Ignore `EventLoopClosed`
            let _ = proxy.send_event(e);
        }
    }
}

impl<TWM: WinitWm, TWC: WndContent<Wm = TWM>> WinitWmCore<TWM, TWC> {
    fn new(wm: TWM) -> Self {
        Self {
            wm,
            event_loop: RefCell::new(Some(EventLoop::with_user_event())),
            should_terminate: Cell::new(false),
            event_loop_wnd_target: Cell::new(None),
            unsend_invoke_events: RefCell::new(LinkedList::new()),
            timer_queue: RefCell::new(TimerQueue::new()),
            wnds: RefCell::new(Pool::new()),
            suppress_request_redraw: Cell::new(false),
        }
    }

    pub fn wm(&self) -> TWM {
        self.wm
    }

    fn create_proxy(&mut self) -> EventLoopProxy<UserEvent<TWM, TWC>> {
        self.event_loop.get_mut().as_ref().unwrap().create_proxy()
    }

    pub fn enter_main_loop(&'static self) -> ! {
        let event_loop = self
            .event_loop
            .replace(None)
            .expect("can't call enter_main_loop twice");

        struct Guard<'a, T>(&'a Cell<Option<T>>);

        impl<T> Drop for Guard<'_, T> {
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

            use winit::event::Event;

            match event {
                Event::WindowEvent { window_id, event } => {
                    self.handle_wnd_evt(window_id, event);
                }
                Event::UserEvent(cb) => {
                    cb(self);
                }
                _ => {}
            }

            loop {
                let e = self.unsend_invoke_events.borrow_mut().pop_front();
                if let Some(e) = e {
                    e(self);
                } else {
                    break;
                }
            }

            if self.should_terminate.get() {
                *control_flow = ControlFlow::Exit;
            } else {
                *control_flow = ControlFlow::Wait;
            }

            // Process delayed invocations
            let timer_queue_cell = &self.timer_queue;

            let mut tasks = ArrayVec::<[_; TimerQueue::<()>::CAPACITY]>::new();
            for (_, task) in timer_queue_cell.borrow_mut().drain_runnable_tasks() {
                // This is safe because the capacity is sufficient to hold
                // all tasks extracted from `timer_queue`
                unsafe { tasks.push_unchecked(task) };
            }

            // Unborrow `timer_queue_cell` first (because `func(self)` may call
            // `invoke_after`), and call the callback functions
            for func in tasks.drain(..) {
                func(self);
            }

            if let Some(next_wakeup) = timer_queue_cell.borrow().suggest_next_wakeup() {
                *control_flow = ControlFlow::WaitUntil(next_wakeup);
            }
        });
    }

    pub fn terminate(&self) {
        self.should_terminate.set(true);
    }

    /// Get a reference to winit's `EventLoop`.
    ///
    /// Returns `None` if the main event loop has already been entered.
    #[allow(dead_code)]
    pub fn event_loop(
        &self,
    ) -> Option<impl std::ops::Deref<Target = EventLoop<UserEvent<TWM, TWC>>> + '_> {
        let x = self.event_loop.borrow();
        if x.is_some() {
            Some(OwningRef::new(x).map(|x| x.as_ref().unwrap()))
        } else {
            None
        }
    }

    /// Call a function using the `EventLoopWindowTarget` supplied by `EventLoop`
    /// or something.
    ///
    /// This possibly immutable borrows `EventLoop`, thus the callback function
    /// must not call `enter_main_loop`.
    pub(super) fn with_event_loop_wnd_target<R>(
        &self,
        f: impl FnOnce(&EventLoopWindowTarget<UserEvent<TWM, TWC>>) -> R,
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

    pub fn invoke(&'static self, cb: impl FnOnce(&'static Self) + 'static) {
        self.unsend_invoke_events
            .borrow_mut()
            .push_back(Box::new(cb));
    }

    pub fn invoke_after(
        &self,
        delay: Range<Duration>,
        f: impl FnOnce(&'static Self) + 'static,
    ) -> HInvokeCore {
        let htask = self
            .timer_queue
            .borrow_mut()
            .insert(delay, Box::new(f))
            .expect("Too many pending delayed invocations");

        HInvokeCore { htask }
    }

    pub fn cancel_invoke(&self, hinv: &HInvokeCore) {
        let _ = self.timer_queue.borrow_mut().remove(hinv.htask);
    }
}
