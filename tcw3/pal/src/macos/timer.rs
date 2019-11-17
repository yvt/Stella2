use cocoa::base::id;
use objc::{msg_send, sel, sel_impl};
use std::{ops::Range, time::Duration};

use super::{utils::with_autorelease_pool, IdRef, Wm};
use crate::iface::Wm as _;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke {
    /// `NSTimer`
    timer: IdRef,
}

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
