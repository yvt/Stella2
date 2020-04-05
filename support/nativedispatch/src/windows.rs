//! Windows backend
use std::{ffi::c_void, ptr::null_mut};
use winapi::{
    shared::minwindef::DWORD,
    um::{threadpoolapiset, winnt},
};

use super::QueuePriority;

mod utils;
use self::utils::assert_win32_ok;

/// Const-initializable `TP_CALLBACK_ENVIRON_V3`
#[allow(dead_code)]
#[repr(C)]
#[derive(Copy, Clone)]
struct TpCallbackEnvironV3 {
    version: winnt::TP_VERSION,
    pool: winnt::PTP_POOL,
    cleanup_group: winnt::PTP_CLEANUP_GROUP,
    cleanup_group_cancel_callback: winnt::PTP_CLEANUP_GROUP_CANCEL_CALLBACK,
    race_dll: winnt::PVOID,
    activation_context: *mut winnt::ACTIVATION_CONTEXT,
    finalization_callback: winnt::PTP_SIMPLE_CALLBACK,
    flags: DWORD,
    callback_priority: winnt::TP_CALLBACK_PRIORITY,
    size: DWORD,
}

/// `InitializeThreadpoolEnvironment`
const TP_ENV_INIT: TpCallbackEnvironV3 = TpCallbackEnvironV3 {
    version: 3,
    pool: null_mut(),
    cleanup_group: null_mut(),
    cleanup_group_cancel_callback: None,
    race_dll: null_mut(),
    activation_context: null_mut(),
    finalization_callback: None,
    flags: 0,
    callback_priority: winnt::TP_CALLBACK_PRIORITY_NORMAL,
    size: std::mem::size_of::<winnt::TP_CALLBACK_ENVIRON_V3>() as _,
};

static TP_ENV_LIST: AssertSendSync<[TpCallbackEnvironV3; 4]> = AssertSendSync([
    // `QueuePriority::High`
    TpCallbackEnvironV3 {
        callback_priority: winnt::TP_CALLBACK_PRIORITY_HIGH,
        ..TP_ENV_INIT
    },
    // `QueuePriority::Medium`
    TpCallbackEnvironV3 {
        callback_priority: winnt::TP_CALLBACK_PRIORITY_NORMAL,
        ..TP_ENV_INIT
    },
    // `QueuePriority::Low`
    TpCallbackEnvironV3 {
        callback_priority: winnt::TP_CALLBACK_PRIORITY_LOW,
        ..TP_ENV_INIT
    },
    // `QueuePriority::Background`
    TpCallbackEnvironV3 {
        callback_priority: winnt::TP_CALLBACK_PRIORITY_LOW,
        ..TP_ENV_INIT
    },
]);

#[derive(Debug, Clone, Copy)]
struct AssertSendSync<T>(T);
unsafe impl<T> Send for AssertSendSync<T> {}
unsafe impl<T> Sync for AssertSendSync<T> {}

#[derive(Debug, Clone, Copy)]
pub struct QueueImpl {
    tp_env: winnt::PTP_CALLBACK_ENVIRON,
}

unsafe impl Send for QueueImpl {}
unsafe impl Sync for QueueImpl {}

impl QueueImpl {
    pub fn global(pri: QueuePriority) -> Self {
        Self {
            tp_env: (&TP_ENV_LIST.0[pri as usize]) as *const _ as _,
        }
    }

    pub fn invoke<F: FnOnce() + Send + 'static>(&self, work: F) {
        let (ctx, func) = ctx_and_fn(work);

        assert_win32_ok(unsafe {
            threadpoolapiset::TrySubmitThreadpoolCallback(func, ctx, self.tp_env)
        });
    }
}

fn ctx_and_fn<F: FnOnce() + Send + 'static>(work: F) -> (*mut c_void, winnt::PTP_SIMPLE_CALLBACK) {
    extern "system" fn tp_callback_trampoline<F: FnOnce() + Send + 'static>(
        _: winnt::PTP_CALLBACK_INSTANCE,
        ctx: *mut c_void,
    ) {
        let work = unsafe { Box::from_raw(ctx as *mut F) };
        work();
    }

    let work = Box::new(work);
    (Box::into_raw(work) as _, Some(tp_callback_trampoline::<F>))
}
