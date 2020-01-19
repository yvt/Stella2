//! libdispatch backend
use dispatch::ffi as disp;
use std::ffi::c_void;

use super::QueuePriority;

#[derive(Debug, Clone, Copy)]
pub struct QueueImpl {
    queue: disp::dispatch_queue_t,
}

unsafe impl Send for QueueImpl {}
unsafe impl Sync for QueueImpl {}

impl QueueImpl {
    pub fn global(pri: QueuePriority) -> Self {
        let queue = unsafe {
            disp::dispatch_get_global_queue(
                match pri {
                    QueuePriority::High => disp::DISPATCH_QUEUE_PRIORITY_HIGH,
                    QueuePriority::Medium => disp::DISPATCH_QUEUE_PRIORITY_DEFAULT,
                    QueuePriority::Low => disp::DISPATCH_QUEUE_PRIORITY_LOW,
                    QueuePriority::Background => disp::DISPATCH_QUEUE_PRIORITY_BACKGROUND,
                },
                0,
            )
        };

        // Global queues are implicitly `'static`.
        //
        // <https://developer.apple.com/library/archive/documentation/General/Conceptual/ConcurrencyProgrammingGuide/OperationQueues/OperationQueues.html>:
        //
        // > Although dispatch queues are reference-counted objects, you do not
        // > need to retain and release the global concurrent queues. Because
        // > they are global to your application, retain and release calls for
        // > these queues are ignored.

        Self { queue }
    }

    pub fn invoke(&self, work: impl FnOnce() + Send + 'static) {
        let (ctx, func) = ctx_and_fn(work);
        unsafe {
            disp::dispatch_async_f(self.queue, ctx, func);
        }
    }
}

fn ctx_and_fn<F: FnOnce() + Send + 'static>(work: F) -> (*mut c_void, disp::dispatch_function_t) {
    extern "C" fn dispatch_work_trampoline<F: FnOnce() + Send + 'static>(ctx: *mut c_void) {
        let work = unsafe { Box::from_raw(ctx as *mut F) };
        work();
    }

    let work = Box::new(work);
    (Box::into_raw(work) as _, dispatch_work_trampoline::<F>)
}
