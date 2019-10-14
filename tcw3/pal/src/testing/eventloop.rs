use std::{
    cell::RefCell,
    collections::LinkedList,
    sync::{
        mpsc::{channel, Receiver, RecvTimeoutError, Sender},
        Mutex,
    },
    thread,
    time::Duration,
};

use super::Wm;
use crate::{prelude::*, MtLock};

mt_lazy_static! {
    static <Wm> ref UNSEND_DISPATCHES: RefCell<LinkedList<Box<dyn FnOnce(Wm)>>> =>
        |_| RefCell::new(LinkedList::new());
}

static DISPATCH_RECV: MtLock<RefCell<Option<Receiver<Dispatch>>>> = MtLock::new(RefCell::new(None));

type Dispatch = Box<dyn FnOnce(Wm) + Send>;

pub type DispatchReceiver = Receiver<Dispatch>;
pub struct DispatchSender(Mutex<Sender<Dispatch>>);

pub fn dispatch_channel() -> (DispatchSender, DispatchReceiver) {
    let (send, recv) = channel();
    (DispatchSender(Mutex::new(send)), recv)
}

impl DispatchSender {
    pub(super) fn invoke_on_main_thread(&self, f: impl FnOnce(Wm) + Send + 'static) {
        self.0.lock().unwrap().send(Box::new(f)).unwrap();
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
        UNSEND_DISPATCHES
            .get_with_wm(self)
            .borrow_mut()
            .push_back(Box::new(f));
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

    pub(super) fn step_timeout(self, timeout: Option<std::time::Duration>) {
        // Check the thread-local queue first because there is no possibility
        // that it can get enqueued by us waiting
        let e = UNSEND_DISPATCHES.get_with_wm(self).borrow_mut().pop_front();
        if let Some(e) = e {
            e(self);
            return;
        }

        // Wait for `invoke_on_main_thread`
        let recv = self.dispatch_receiver();
        let result = if let Some(timeout) = timeout {
            match recv.recv_timeout(timeout) {
                Ok(x) => Some(x),
                Err(RecvTimeoutError::Timeout) => return,
                Err(RecvTimeoutError::Disconnected) => None,
            }
        } else {
            recv.recv().ok()
        };

        if let Some(fun) = result {
            fun(self);
            return;
        }

        // We are not receving events anymore, sleep indefinitely
        loop {
            thread::sleep(Duration::from_secs(256));
        }
    }
}
