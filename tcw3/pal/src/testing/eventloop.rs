use log::{trace, warn};
use std::{
    cell::RefCell,
    collections::LinkedList,
    ops::Range,
    sync::{
        mpsc::{channel, Receiver, RecvTimeoutError, Sender},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use super::Wm;
use crate::{
    prelude::MtLazyStatic,
    timerqueue::{HTask, TimerQueue},
    MtLock, MtSticky,
};

static UNSEND_DISPATCHES: MtSticky<RefCell<LinkedList<Box<dyn FnOnce(Wm)>>>> = {
    // This is safe because the created value does not contain an actual
    // unsendable content (`Box<dyn FnOnce(Wm)>`) yet
    unsafe { MtSticky::new_unchecked(RefCell::new(LinkedList::new())) }
};

static DISPATCH_RECV: MtLock<RefCell<Option<Receiver<Dispatch>>>> = MtLock::new(RefCell::new(None));

mt_lazy_static! {
    static <Wm> ref TIMER_QUEUE: RefCell<TimerQueue<Box<dyn FnOnce(Wm)>>> =>
        |_| RefCell::new(TimerQueue::new());
}

type Dispatch = Box<dyn FnOnce(Wm) + Send>;

pub type DispatchReceiver = Receiver<Dispatch>;
pub struct DispatchSender(Mutex<Sender<Dispatch>>);

pub type HInvoke = HTask;

pub fn dispatch_channel() -> (DispatchSender, DispatchReceiver) {
    let (send, recv) = channel();
    (DispatchSender(Mutex::new(send)), recv)
}

impl DispatchSender {
    pub(super) fn invoke_on_main_thread(&self, f: impl FnOnce(Wm) + Send + 'static) {
        let boxed: Dispatch = Box::new(f);
        trace!("invoke_on_main_thread({:?})", (&*boxed) as *const _);
        self.0.lock().unwrap().send(boxed).unwrap();
    }
}

impl Wm {
    pub(super) fn set_dispatch_receiver(self, recv: DispatchReceiver) {
        let mut recv_cell = DISPATCH_RECV.get_with_wm(self).borrow_mut();
        assert!(recv_cell.is_none());
        *recv_cell = Some(recv);
    }

    fn dispatch_receiver(self) -> impl std::ops::Deref<Target = Receiver<Dispatch>> {
        use owning_ref::OwningRef;
        OwningRef::new(DISPATCH_RECV.get_with_wm(self).borrow()).map(|refr| {
            // If `Backend::Testing` is active, we should never observe
            // `DISPATCH_RECV` containing `None` because it's set before the
            // main thread enters the main loop.
            refr.as_ref()
                .expect("Could not get a dispatch receiver. Perhaps the native backend is in use?")
        })
    }

    pub(super) fn invoke_unsend(self, f: impl FnOnce(Self) + 'static) {
        let boxed: Box<dyn FnOnce(Wm)> = Box::new(f);
        trace!("invoke_unsend({:?})", (&*boxed) as *const _);
        UNSEND_DISPATCHES
            .get_with_wm(self)
            .borrow_mut()
            .push_back(boxed);
    }

    pub(super) fn invoke_after(
        self,
        delay: Range<Duration>,
        f: impl FnOnce(Self) + 'static,
    ) -> HInvoke {
        let boxed: Box<dyn FnOnce(Wm)> = Box::new(f);
        trace!("invoke_after({:?}, {:?})", delay, (&*boxed) as *const _);

        TIMER_QUEUE
            .get_with_wm(self)
            .borrow_mut()
            .insert(delay, boxed)
            .map_err(|e| {
                warn!(
                    "invoke_after failed because \
                     there are too many pending delayed invocations."
                );
                e
            })
            .expect("Too many pending delayed invocations")
    }

    pub(super) fn cancel_invoke(self, hinv: &HInvoke) {
        if let Some(boxed) = TIMER_QUEUE.get_with_wm(self).borrow_mut().remove(*hinv) {
            trace!(
                "cancel_invoke({:?}) cancelled {:?}",
                hinv,
                (&*boxed) as *const _
            );
        } else {
            trace!("cancel_invoke({:?}) did not cancel anything", hinv);
        }
    }

    pub(super) fn enter_main_loop(self) {
        while let Ok(fun) = self.dispatch_receiver().recv() {
            fun(self);

            // `fun` might push dispatches to `UNSEND_DISPATCHES`
            self.step_unsend();
        }
    }

    pub(super) fn step_unsend(self) {
        loop {
            let e = UNSEND_DISPATCHES.get_with_wm(self).borrow_mut().pop_front();
            if let Some(e) = e {
                e(self);
            } else {
                break;
            }
        }
    }

    pub(super) fn step_timeout(self, mut timeout: Option<std::time::Duration>) {
        // Check the thread-local queue first because there is no possibility
        // that it can get enqueued by us waiting
        let e = UNSEND_DISPATCHES.get_with_wm(self).borrow_mut().pop_front();
        if let Some(e) = e {
            e(self);
            return;
        }

        // Want to iterate at least once, so don't use `while timeout != ...` here
        loop {
            // And then check the thread-local delayed invocations for the same reason
            let e = {
                let mut timer_queue = TIMER_QUEUE.get_with_wm(self).borrow_mut();
                if let Some(htask) = timer_queue.runnable_tasks().nth(0) {
                    Some(timer_queue.remove(htask).unwrap())
                } else {
                    None
                }
            };
            if let Some(e) = e {
                e(self);
                return;
            }

            // Maybe we have a runnable delayed invocation if we wait long enough...
            // But we shouldn't wait longer than the given `timeout`.
            let recv_timeout = {
                let next = TIMER_QUEUE
                    .get_with_wm(self)
                    .borrow()
                    .suggest_next_wakeup()
                    .map(|instant| instant.saturating_duration_since(Instant::now()));

                match (timeout, next) {
                    (Some(x), Some(y)) => Some(x.min(y)),
                    (Some(x), None) | (None, Some(x)) => Some(x),
                    (None, None) => None,
                }
            };

            if let (Some(timeout), Some(recv_timeout)) = (&mut timeout, recv_timeout) {
                *timeout -= recv_timeout;
            }

            // Wait for `invoke_on_main_thread`
            let recv = self.dispatch_receiver();
            let result = if let Some(recv_timeout) = recv_timeout {
                match recv.recv_timeout(recv_timeout) {
                    Ok(x) => Some(x),
                    Err(RecvTimeoutError::Timeout) => {
                        if timeout == Some(Duration::from_secs(0)) {
                            return;
                        }

                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => None,
                }
            } else {
                recv.recv().ok()
            };

            if let Some(fun) = result {
                fun(self);
                return;
            }

            // `recv` is disconnected, pretend like it's not
            if let Some(recv_timeout) = recv_timeout {
                // (To be precise, this may be supposed to be shorter if `recv` gets
                // disconnected whilst we are waiting for `recv_timeout` to complete)
                thread::sleep(recv_timeout);

                if timeout == Some(Duration::from_secs(0)) {
                    return;
                }
            } else {
                // We are not receving events anymore, sleep indefinitely
                loop {
                    thread::sleep(Duration::from_secs(256));
                }
            }
        }
    }

    pub(super) fn eradicate_events(self) {
        let queue = UNSEND_DISPATCHES.get_with_wm(self);
        if !queue.borrow().is_empty() {
            warn!(
                "Executing {} unprocessed unsend dispatch(es)",
                queue.borrow().len()
            );

            let mut num_actually_dropped = 0;
            loop {
                let e = queue.borrow_mut().pop_front();
                if let Some(e) = e {
                    // `queue` must be unborrowed before dropping `e` because
                    // `e`'s drop handler might generate even more dispatches.
                    num_actually_dropped += 1;
                    drop(e);
                } else {
                    break;
                }
            }

            warn!(
                "Executed {} unprocessed unsend dispatch(es)",
                num_actually_dropped
            );
        }
    }
}
