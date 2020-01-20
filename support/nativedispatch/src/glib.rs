//! glib backend
use std::{ffi::c_void, os::raw::c_int, ptr::null_mut};

use super::QueuePriority;

lazy_static::lazy_static! {
    static ref THREAD_POOL: AssertSendSync<*mut glib_sys::GThreadPool> = unsafe {
        let thread_pool = glib_sys::g_thread_pool_new(
            Some(thread_pool_process_work),
            null_mut(), // user data
            -1,
            0, // non-exclusive
            null_mut(), // [out] error
        );
        assert!(!thread_pool.is_null());

        // Make sure high-priority tasks are processed first
        glib_sys::g_thread_pool_set_sort_function(
            thread_pool,
            Some(thread_pool_ord),
            null_mut(), // user data
        );

        AssertSendSync(thread_pool)
    };
}

#[derive(Debug, Clone, Copy)]
struct AssertSendSync<T>(T);
unsafe impl<T> Send for AssertSendSync<T> {}
unsafe impl<T> Sync for AssertSendSync<T> {}

#[repr(C)]
struct Work<T> {
    pri: QueuePriority,
    func: unsafe fn(*mut Work<()>),
    data: T,
}

unsafe extern "C" fn thread_pool_process_work(data: *mut c_void, _user_data: *mut c_void) {
    let data = data as *mut Work<()>;

    // This is safe because `Work<()>` is a prefix of any instance of
    // `Work<T>`
    let func = (*data).func;

    // This is safe because we are passing `data` to its associated `func`
    func(data);
}

unsafe extern "C" fn thread_pool_ord(
    x: *const c_void,
    y: *const c_void,
    _user_data: *mut c_void,
) -> c_int {
    // This is safe because `Work<()>` is a prefix of any instance of
    // `Work<T>`
    let work1 = &*(x as *const Work<()>);
    let work2 = &*(y as *const Work<()>);

    work1.pri as c_int - work2.pri as c_int
}

#[derive(Debug, Clone, Copy)]
pub struct QueueImpl {
    pool: AssertSendSync<*mut glib_sys::GThreadPool>,
    pri: QueuePriority,
}

unsafe impl Send for QueueImpl {}
unsafe impl Sync for QueueImpl {}

impl QueueImpl {
    pub fn global(pri: QueuePriority) -> Self {
        Self {
            pool: *THREAD_POOL,
            pri,
        }
    }

    pub fn invoke<F: FnOnce() + Send + 'static>(&self, data: F) {
        let work: Box<Work<F>> = Box::new(Work {
            pri: self.pri,
            func: |work_untyped: *mut Work<()>| {
                let work = unsafe { Box::from_raw(work_untyped as *mut Work<F>) };
                (work.data)();
            },
            data,
        });

        let work_ptr = Box::into_raw(work);

        let success =
            unsafe { glib_sys::g_thread_pool_push(self.pool.0, work_ptr as _, null_mut()) };
        assert_ne!(success, 0);
    }
}
