use cocoa::base::id;
use objc::{msg_send, sel, sel_impl};
use std::{ops::Range, time::Duration};

use super::{utils::with_autorelease_pool, IdRef, Wm};
use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke {
    /// `NSTimer`
    timer: IdRef,
}

// This is safe because
//
// 1. `NSTimer` is explicitly described as thread-safe:
//
//    <https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/ThreadSafetySummary/ThreadSafetySummary.html>:
//
//    > The following classes and functions are generally considered to be
//    > thread-safe. You can use the same instance from multiple threads without
//    > first acquiring a lock.
//
// 2. `tcw_invoke_cancel` is called when the timer's target object is
//    released, which happens when the timer is invalidated. The invalidation
//    occurs because of one of the following causes, both of which take place
//    in the main thread:
//
//     - The timer fires. This happens in the same thread as the one where the
//       timer is created, which is the main thread, where `Wm::invoke_after` is
//       called.
//
//     - The timer is explicitly invalidated by `[NSTimer invalidate]`.  This
//       is only allowed through `Wm::cancel_invoke`, which requires `Wm`.
//
unsafe impl Send for HInvoke {}
unsafe impl Sync for HInvoke {}

/// Implements `Wm::invoke_after`.
pub fn invoke_after(_: Wm, delay: Range<Duration>, f: impl FnOnce(Wm) + 'static) -> HInvoke {
    let start = delay.start.as_secs_f64();
    let end = delay.end.as_secs_f64();
    let ud: TCWInvokeUserDataInner = Box::into_raw(Box::new(f));
    let timer = with_autorelease_pool(|| {
        let timer = unsafe { TCWInvokeAfter(start, end - start, std::mem::transmute(ud)) };

        // `timer` is an autorelease ref, so increase the ref count
        IdRef::retain(timer)
    });
    HInvoke { timer }
}

/// Implements `Wm::cancel_invoke`.
pub fn cancel_invoke(_: Wm, hinvoke: &HInvoke) {
    unsafe {
        let () = msg_send![*hinvoke.timer, invalidate];
    }
}

extern "C" {
    fn TCWInvokeAfter(delay: f64, tolerance: f64, ud: TCWInvokeUserData) -> id;
}

/// The FFI-safe representation of `TCWInvokeUserDataInner`
#[repr(C)]
struct TCWInvokeUserData {
    __data: *mut std::ffi::c_void,
    __vtable: *mut std::ffi::c_void,
}
type TCWInvokeUserDataInner = *mut dyn FnOnce(Wm);

#[no_mangle]
unsafe extern "C" fn tcw_invoke_fire(ud: TCWInvokeUserData) {
    let ud: TCWInvokeUserDataInner = std::mem::transmute(ud);
    let func = Box::from_raw(ud);
    debug_assert!(Wm::is_main_thread(), "ud was sent to a non-main thread");
    func(Wm::global_unchecked());
}

#[no_mangle]
unsafe extern "C" fn tcw_invoke_cancel(ud: TCWInvokeUserData) {
    let ud: TCWInvokeUserDataInner = std::mem::transmute(ud);
    debug_assert!(Wm::is_main_thread(), "ud was sent to a non-main thread");
    drop(Box::from_raw(ud));
}
