//! Extends `Wm` for interoperability with futures (`std::future::Future`).
use futures::task::{FutureObj, LocalFutureObj, LocalSpawn, Spawn, SpawnError};
use leakypool::{LeakyPool, PoolPtr};
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    fmt,
    future::Future,
    ops::Range,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    time::Duration,
};

use crate::{prelude::*, HInvoke, MtSticky, Wm};

/// Extends [`Wm`] for interoperability with futures (`std::future::Future`).
///
/// Unfortunately, this can't be generalized over all implementations of
/// [`Wm`](crate::iface::Wm) trait because static items cannot be made generic.
///
/// [`Wm`]: crate::Wm
pub trait WmFuturesExt {
    /// Get a handle implementing [`Spawn`].
    ///
    /// [`Spawn`]: futures::task::Spawn
    fn spawner(self) -> WmSpawner;

    /// Sleep for the specified amount of time with some tolerance specified
    /// in the form of a range.
    ///
    /// This method is a "futures" version of [`Wm::invoke_after`] and is
    /// internally implemented by this.
    ///
    /// [`Wm::invoke_after`]: crate::iface::Wm::invoke_after
    fn sleep(self, dur: Range<Duration>) -> Sleep;
}

impl WmFuturesExt for Wm {
    fn spawner(self) -> WmSpawner {
        WmSpawner { wm: self }
    }

    fn sleep(self, dur: Range<Duration>) -> Sleep {
        Sleep::new(self, dur)
    }
}

// ============================================================================

static TASKS: MtSticky<RefCell<LeakyPool<Task>>> = {
    // `Task` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(LeakyPool::new())) }
};

struct Task {
    /// This future is moved out only when `pend_task` is making a progress.
    future: Cell<Option<LocalFutureObj<'static, ()>>>,
}

#[derive(Debug, Copy, Clone)]
pub struct WmSpawner {
    wm: Wm,
}

impl Spawn for WmSpawner {
    fn spawn_obj(&self, future: FutureObj<'static, ()>) -> Result<(), SpawnError> {
        self.spawn_local_obj(future.into())
    }
}

impl LocalSpawn for WmSpawner {
    fn spawn_local_obj(&self, future: LocalFutureObj<'static, ()>) -> Result<(), SpawnError> {
        let mut tasks = TASKS.get_with_wm(self.wm).borrow_mut();

        let task_id = tasks.allocate(Task {
            future: Cell::new(Some(future)),
        });

        self.wm.invoke(move |wm| {
            pend_task(wm, task_id);
        });

        Ok(())
    }
}

/// Progress a task. The call stack should not include another activation of
/// `pend_task`.
fn pend_task(wm: Wm, task_id: PoolPtr<Task>) {
    let mut future: LocalFutureObj<'static, ()>;

    if let Some(task) = TASKS.get_with_wm(wm).borrow_mut().get_mut(task_id) {
        future = task.future.take().unwrap();
    } else {
        // already-completed task, do not enqueue
        return;
    }

    let waker = wm_waker(task_id);
    let result = Pin::new(&mut future).poll(&mut Context::from_waker(&waker));

    let mut tasks = TASKS.get_with_wm(wm).borrow_mut();

    match result {
        Poll::Ready(()) => {
            // Delete the completed task
            tasks.deallocate(task_id).unwrap();
        }
        Poll::Pending => {
            // Put `future` back
            tasks[task_id].future.set(Some(future));

            // `pend_task` will be called again sometime...
        }
    }
}

fn wm_waker(task_id: PoolPtr<Task>) -> Waker {
    fn wm_raw_waker(task_id: PoolPtr<Task>) -> RawWaker {
        let vtable: &'static RawWakerVTable =
            &RawWakerVTable::new(wm_waker_clone, wm_waker_wake, wm_waker_wake, wm_waker_drop);

        RawWaker::new(task_id.into_raw().as_ptr() as *const (), vtable)
    }

    unsafe fn data_to_task_id(data: *const ()) -> PoolPtr<Task> {
        PoolPtr::from_raw(NonNull::new_unchecked(data as *mut ()))
    }

    unsafe fn wm_waker_clone(data: *const ()) -> RawWaker {
        wm_raw_waker(data_to_task_id(data))
    }
    unsafe fn wm_waker_wake(data: *const ()) {
        let task_id = data_to_task_id(data);

        Wm::invoke_on_main_thread(move |wm| {
            pend_task(wm, task_id);
        });
    }
    unsafe fn wm_waker_drop(_: *const ()) {}

    unsafe { Waker::from_raw(wm_raw_waker(task_id)) }
}

// ============================================================================

/// Represents a sleep operation.
#[derive(Clone)]
pub struct Sleep {
    inner: Rc<SleepInner>,
}

#[derive(Debug, Clone, Copy)]
pub struct SleepCancelled;

impl Sleep {
    fn new(wm: Wm, dur: Range<Duration>) -> Self {
        let inner = Rc::new(SleepInner {
            wm,
            waker: Cell::new(None),
            result: Cell::new(None),
            hinvoke: UnsafeCell::new(None),
        });

        let inner_weak = Rc::downgrade(&inner);
        let hinvoke = wm.invoke_after(dur, move |_| {
            if let Some(inner) = inner_weak.upgrade() {
                debug_assert!(inner.result.get().is_none());

                inner.result.set(Some(Ok(())));
                if let Some(waker) = inner.waker.take() {
                    waker.wake();
                }
            }
        });

        unsafe {
            *inner.hinvoke.get() = Some(hinvoke);
        }

        Self { inner }
    }

    /// Cancel the operation.
    ///
    /// Returns `true` if the operation is successfully cancelled; `false`
    /// otherwise, e.g., because the operation is already complete or cancelled.
    pub fn cancel(&self) -> bool {
        self.inner.cancel()
    }

    /// Poll the state without registering a `Waker`.
    pub fn poll_without_context(&self) -> Poll<Result<(), SleepCancelled>> {
        if let Some(result) = self.inner.result.get() {
            Poll::Ready(result)
        } else {
            Poll::Pending
        }
    }
}

struct SleepInner {
    wm: Wm,
    waker: Cell<Option<Waker>>,
    result: Cell<Option<Result<(), SleepCancelled>>>,
    /// Inited when constructing `Sleep`. Safe to deref immutably because it
    /// only changes in one way: `None` to `Some(x)`.
    hinvoke: UnsafeCell<Option<HInvoke>>,
}

impl fmt::Debug for Sleep {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sleep")
            .field("wm", &self.inner.wm)
            .field("result", &self.inner.result)
            .field(
                "hinvoke",
                unsafe { &*self.inner.hinvoke.get() }.as_ref().unwrap(),
            )
            .finish()
    }
}

impl Future for Sleep {
    type Output = Result<(), SleepCancelled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if let Some(result) = self.inner.result.get() {
            Poll::Ready(result)
        } else {
            let new_waker = cx.waker();

            if let Some(old_waker) = self.inner.waker.take() {
                if old_waker.will_wake(new_waker) {
                    self.inner.waker.set(Some(old_waker));
                    return Poll::Pending;
                }
            }

            self.inner.waker.set(Some(new_waker.clone()));
            Poll::Pending
        }
    }
}

impl Drop for SleepInner {
    fn drop(&mut self) {
        if self.result.get().is_none() {
            // There is no future or task woken up by this, so just cancel
            // the invocation.
            // `hinvoke` is `None` if `invoke_after` panics.
            if let Some(hinvoke) = unsafe { &*self.hinvoke.get() }.as_ref() {
                self.wm.cancel_invoke(hinvoke);
            }
        }
    }
}

impl SleepInner {
    fn cancel(&self) -> bool {
        if self.result.get().is_none() {
            let hinvoke = unsafe { &*self.hinvoke.get() }.as_ref().unwrap();
            self.wm.cancel_invoke(hinvoke);

            self.result.set(Some(Err(SleepCancelled)));
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }

            true
        } else {
            false
        }
    }
}
