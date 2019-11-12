//! Extends `Wm` for interoperability with futures (`std::future::Future`).
use futures::task::{FutureObj, LocalFutureObj, LocalSpawn, Spawn, SpawnError};
use iterpool::{Pool, PoolPtr};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use crate::{iface::Wm as _, MtSticky, Wm};

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
}

impl WmFuturesExt for Wm {
    fn spawner(self) -> WmSpawner {
        WmSpawner { wm: self }
    }
}

// ============================================================================

static TASKS: MtSticky<RefCell<Pool<Task>>> = {
    // `Task` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(Pool::new())) }
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
fn pend_task(wm: Wm, task_id: PoolPtr) {
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

fn wm_waker(task_id: PoolPtr) -> Waker {
    const fn wm_raw_waker(task_id: PoolPtr) -> RawWaker {
        let vtable: &'static RawWakerVTable =
            &RawWakerVTable::new(wm_waker_clone, wm_waker_wake, wm_waker_wake, wm_waker_drop);

        RawWaker::new(task_id.0.get() as *const (), vtable)
    }

    unsafe fn data_to_task_id(data: *const ()) -> PoolPtr {
        PoolPtr(std::num::NonZeroUsize::new_unchecked(data as usize))
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
