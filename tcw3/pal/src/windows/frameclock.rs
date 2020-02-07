use atom2::SetOnceAtom;
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Condvar, Mutex,
    },
};
use winapi::um::dwmapi::DwmFlush;

use super::Wm;
use crate::{iface::Wm as _, MtSticky};

struct DisplayLink {
    inner: SetOnceAtom<Box<DisplayLinkInner>>,
}

struct DisplayLinkInner {
    cv: Condvar,
    mutex: Mutex<()>,
    /// Put it outside `mutex` to avoid locking whenever possible
    running: AtomicBool,
}

impl DisplayLink {
    pub const fn new() -> Self {
        Self {
            inner: SetOnceAtom::empty(),
        }
    }

    /// Start the display link if not running.
    ///
    // If the method is called for the first time, `handler` is registered
    // as the handler function.
    pub fn start(&'static self, mut handler: impl FnMut() + Send + 'static) {
        let inner = if let Some(inner) = self.inner.as_inner_ref() {
            inner
        } else {
            let inner = DisplayLinkInner {
                cv: Condvar::new(),
                mutex: Mutex::new(()),
                running: AtomicBool::new(false),
            };

            // Racy (leaky) initialization
            std::mem::forget(self.inner.store(Some(Box::new(inner))));

            let inner = self.inner.as_inner_ref().unwrap();

            std::thread::spawn(move || {
                loop {
                    // Suspend if the display link is requested to stop.
                    if !inner.running.load(Ordering::Relaxed) {
                        let mut guard = inner.mutex.lock().unwrap();
                        while !inner.running.load(Ordering::Relaxed) {
                            guard = inner.cv.wait(guard).unwrap();
                        }
                    }

                    // This is rather surprising, but actually what Firefox
                    // seems to do.
                    unsafe { DwmFlush() };

                    handler();
                }
            });

            inner
        };

        // Acquire a lock only if this is effectful
        if !inner.running.load(Ordering::Relaxed) {
            inner.running.store(true, Ordering::Relaxed);
            inner.cv.notify_one();
        }
    }

    /// Stop the display link.
    pub fn stop(&self) {
        if let Some(inner) = self.inner.as_inner_ref() {
            inner.running.store(false, Ordering::Relaxed);
        }
    }
}

pub struct FrameClockManager<T: 'static> {
    pending_clients: MtSticky<RefCell<Vec<T>>>,
    dl: DisplayLink,
}

pub trait FrameClockClient {
    fn set_pending(&mut self, x: bool);
    fn is_pending(&mut self) -> bool;
    fn handle_frame_clock(&mut self, wm: Wm);
}

impl<T: FrameClockClient + 'static> FrameClockManager<T> {
    pub const fn new() -> Self {
        Self {
            // This is safe because there's nothing `!Send` in it yet
            pending_clients: unsafe { MtSticky::new_unchecked(RefCell::new(Vec::new())) },
            dl: DisplayLink::new(),
        }
    }

    /// Register a client to be signalled once. No-op if the client is already
    /// registered.
    ///
    /// This method is used to implement `request_update_ready_wnd`.
    pub fn register(&'static self, wm: Wm, mut client: T) {
        if client.is_pending() {
            return;
        }
        client.set_pending(true);

        self.pending_clients
            .get_with_wm(wm)
            .borrow_mut()
            .push(client);

        let clients: &'static _ =
            Box::leak(Box::new(MtSticky::with_wm(wm, RefCell::new(Vec::new()))));

        self.dl.start(move || {
            // Handle the signal
            Wm::invoke_on_main_thread(move |wm| {
                // Since we are using `invoke_on_main_thread`, we are near the
                // top-level of the call stack. Borrowing should succeed.
                let mut clients = clients.get_with_wm(wm).borrow_mut();
                let pending_clients = self.pending_clients.get_with_wm(wm);

                // Move the elements of `self.pending_clients` to `clients`.
                // Use the ping pong buffers technique to reduce the number of
                // memory allocation operations.
                clients.clear();
                std::mem::swap(&mut *pending_clients.borrow_mut(), &mut *clients);
                for client in clients.iter_mut() {
                    // The `pending` flag should be set iff `client` is
                    // included in `self.clients`. So it should be cleared now.
                    client.set_pending(false);
                }

                // Call the handler
                for client in clients.iter_mut() {
                    client.handle_frame_clock(wm);
                }

                // If `register` was called in `handle_frame_clock`,
                // keep the display link running. Otherwise, stop the display
                // link.
                if pending_clients.borrow().len() == 0 {
                    self.dl.stop();
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    #[test]
    fn display_link_sane_rate() {
        static DL: DisplayLink = DisplayLink::new();
        static COUNT: AtomicUsize = AtomicUsize::new(0);

        DL.start(|| {
            COUNT.fetch_add(1, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_secs(1));
        DL.stop();

        // shouldn't fire too fast
        assert!(dbg!(COUNT.load(Ordering::Relaxed)) < 100000);

        // should fire at least once
        assert!(COUNT.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn display_link_stop() {
        static DL: DisplayLink = DisplayLink::new();
        static COUNT: AtomicUsize = AtomicUsize::new(0);

        DL.start(|| {
            COUNT.fetch_add(1, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_millis(200));
        DL.stop();
        std::thread::sleep(Duration::from_millis(200));

        COUNT.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(500));

        // shouldn't fire after stopiing
        assert_eq!(COUNT.load(Ordering::Relaxed), 0);
    }
}
