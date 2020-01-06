use iterpool::{Pool, PoolPtr};
use std::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
    ops::Range,
    ptr::null_mut,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use wchar::wch_c;
use winapi::{
    shared::{
        basetsd::UINT_PTR,
        minwindef::{LPARAM, LRESULT, UINT, WPARAM},
        ntdef::HANDLE,
        windef::HWND,
    },
    um::{
        handleapi::{CompareObjectHandles, DuplicateHandle},
        libloaderapi::GetModuleHandleW,
        processthreadsapi::{GetCurrentProcess, GetCurrentThread},
        winnt::DUPLICATE_SAME_ACCESS,
        winuser::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, KillTimer,
            PostMessageW, PostQuitMessage, RegisterClassW, SetCoalescableTimer, TranslateMessage,
            CW_USEDEFAULT, TIMERV_NO_COALESCING, WM_TIMER, WM_USER, WNDCLASSW,
        },
    },
};

use super::{Wm, window};
use crate::{iface::Wm as WmTrait, MtSticky};

/// `HWND`
static MSG_HWND: AtomicUsize = AtomicUsize::new(0);

/// `HANDLE`
static MAIN_HTHREAD: AtomicUsize = AtomicUsize::new(0);

static TIMERS: MtSticky<RefCell<Pool<Timer>>, Wm> = {
    // `Timer` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(Pool::new())) }
};

struct Timer {
    token: u64,
    handler: Box<dyn FnOnce(Wm)>,
}

static NEXT_TIMER_TOKEN: MtSticky<Cell<u64>> = MtSticky::new(Cell::new(0));

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HInvoke {
    ptr: PoolPtr,
    token: u64,
}

pub fn is_main_thread() -> bool {
    let main_hthread = MAIN_HTHREAD.load(Ordering::Acquire) as HANDLE;
    if main_hthread == 0 as HANDLE {
        return is_main_thread_slow();
    }

    let cur_hthread = unsafe { GetCurrentThread() };

    // This is unfortunate for Win 8.1 (or earlier) users. But Win 7's extended
    // support period is almost over (at the point of writing) and Win 8/8.1
    // aren't used as much. The mainstream support for them already ended long
    // time ago.
    let eq = unsafe { CompareObjectHandles(main_hthread, cur_hthread) };

    eq != 0
}

#[cold]
fn is_main_thread_slow() -> bool {
    init_main_thread();

    // If the call succeeds, we know we are currently in the main thread
    true
}

fn get_msg_hwnd() -> HWND {
    if MAIN_HTHREAD.load(Ordering::Acquire) == 0 {
        panic!("No main thread");
    }

    let msg_hwnd = MSG_HWND.load(Ordering::Relaxed) as HWND;
    debug_assert_ne!(msg_hwnd, null_mut());

    msg_hwnd
}

fn get_msg_hwnd_with_wm(_: Wm) -> HWND {
    // Owning `Wm` means a main thread is already initialized, so
    // `MSG_HWND` should already have a valid window handle
    let msg_hwnd = MSG_HWND.load(Ordering::Relaxed) as HWND;
    debug_assert_ne!(msg_hwnd, null_mut());

    msg_hwnd
}

pub fn invoke_on_main_thread(f: Box<dyn FnOnce(Wm) + Send>) {
    invoke_inner(get_msg_hwnd(), f);
}

pub fn invoke(wm: Wm, f: Box<dyn FnOnce(Wm) + Send>) {
    invoke_inner(get_msg_hwnd_with_wm(wm), f);
}

fn invoke_inner(hwnd: HWND, f: Box<dyn FnOnce(Wm) + Send>) {
    // TODO: find a way to avoid double boxing
    let boxed: InvokePayload = Box::new(f);

    unsafe {
        PostMessageW(hwnd, MSG_WND_WM_INVOKE, Box::into_raw(boxed) as WPARAM, 0);
    }
}

pub fn invoke_after(wm: Wm, delay: Range<Duration>, f: Box<dyn FnOnce(Wm)>) -> HInvoke {
    let delay_ms = delay.start.as_millis() as u32..delay.end.as_millis() as u32;
    debug_assert!(delay_ms.start <= delay_ms.end);

    let hwnd = get_msg_hwnd_with_wm(wm);

    let next_timer_token = NEXT_TIMER_TOKEN.get_with_wm(wm);
    let token = next_timer_token.get();
    next_timer_token.set(token + 1);

    let ptr = TIMERS
        .get_with_wm(wm)
        .borrow_mut()
        .allocate(Timer { token, handler: f });

    // Derive a timer ID from the `PoolPtr`. the timer ID must be nonzero,
    // which is upheld by the fact that `PoolPtr` is backed by `NonZeroUsize`.
    let timer_id = ptr.0.get();
    debug_assert_ne!(timer_id, 0);

    // `SetCoalescableTimer` needs Win 8 or later
    assert_ne!(
        unsafe {
            SetCoalescableTimer(
                hwnd,
                timer_id,
                delay_ms.start as UINT,
                None, // use window proc
                if delay_ms.end <= delay_ms.start {
                    TIMERV_NO_COALESCING
                } else {
                    // Must be less than or equal to `0x7FFFFFF5`, or it will have
                    // a different meaning
                    delay_ms.end - delay_ms.start
                },
            )
        },
        0
    );

    HInvoke { ptr, token }
}

pub fn cancel_invoke(wm: Wm, hinvoke: &HInvoke) {
    let mut timers = TIMERS.get_with_wm(wm).borrow_mut();

    // `PoolPtr` can be reused, so use `token` for a strict identity check
    if timers.get(hinvoke.ptr).map(|t| t.token) != Some(hinvoke.token) {
        return;
    }

    timers.deallocate(hinvoke.ptr).unwrap();
    drop(timers);

    let hwnd = get_msg_hwnd_with_wm(wm);

    // Derive a timer ID from the `PoolPtr` (must be done in the same way
    // as `invoke_after` does).
    let timer_id = hinvoke.ptr.0.get();

    unsafe {
        KillTimer(hwnd, timer_id);
    }

    // <https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-killtimer>:
    //
    // > The KillTimer function does not remove WM_TIMER messages already
    // > posted to the message queue.
    //
    // This is problematic. Let's suppose we have a timer and at some point we
    // kill it. The above sentence tells us that the corresponding timer event
    // may already be enqueued, in which case we will observe the timer event
    // with this timer ID.
    //
    // Now, we create another timer and let's suppose the system decides to
    // reuse the same timer ID. When we receive a timer event with this timer
    // ID, we cannot tell if it refers to the original timer or the new timer.
    //
    // Since there doesn't seem to be a reasonable work-around, I'm going to
    // just assume that the timer ID reuse doesn't happen too often, and
    // should it happen (in this case, the second timer fires too early), it
    // won't cause a serious problem.
}

pub fn enter_main_loop() {
    // Make sure the main thread is initialized
    is_main_thread();

    // The check is optional as far as memory safety is concerned
    debug_assert!(is_main_thread());

    loop {
        let mut msg = MaybeUninit::uninit();

        match unsafe { GetMessageW(msg.as_mut_ptr(), null_mut(), 0, 0) } {
            0 => {
                // Received `WM_QUIT`
                return;
            }
            -1 => {
                panic!("GetMessageW failed");
            }
            _ => {
                unsafe {
                    TranslateMessage(msg.as_ptr());
                }
                unsafe {
                    DispatchMessageW(msg.as_ptr());
                }
            }
        }
    }
}

pub fn terminate() {
    unsafe {
        PostQuitMessage(0);
    }
}

const MSG_WND_CLASS: &[u16] = wch_c!("TcwMsgWnd");

/// Message sent by `invoke_on_main_thread`.
const MSG_WND_WM_INVOKE: UINT = WM_USER;

type InvokePayload = Box<Box<dyn FnOnce(Wm) + Send>>;

/// Configures the current thread as a main thread. Panics if there is already
/// a main thread.
#[cold]
fn init_main_thread() {
    let hinstance = unsafe { GetModuleHandleW(null_mut()) };

    // Create a window class for the message-only window
    let wnd_class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(msg_wnd_proc),
        hInstance: hinstance,
        lpszClassName: MSG_WND_CLASS.as_ptr(),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null_mut(),
    };

    unsafe { RegisterClassW(&wnd_class) };

    // Create a message-only window
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            MSG_WND_CLASS.as_ptr(),
            null_mut(), // title
            0,          // window style
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            null_mut(),
            null_mut(),
            hinstance,
            null_mut(),
        )
    };

    assert_ne!(hwnd, null_mut());

    // Get the handle for the current thread. `GetCurrentThread` returns a
    // pseudo handle, which is converted to a "real" handle by
    // `DuplicateHandle`.
    let cur_pseudo_hthread = unsafe { GetCurrentThread() };

    let cur_hprocess = unsafe { GetCurrentProcess() };
    let mut cur_hthread = MaybeUninit::uninit();
    assert_ne!(
        unsafe {
            DuplicateHandle(
                cur_hprocess,
                cur_pseudo_hthread, // source handle
                cur_hprocess,
                cur_hthread.as_mut_ptr(), // target handle
                0, // desired access - ignored because of `DUPLICATE_SAME_ACCESS`
                0, // do not inherit
                DUPLICATE_SAME_ACCESS,
            )
        },
        0
    );

    let cur_hthread = unsafe { cur_hthread.assume_init() };
    assert_ne!(cur_hthread, null_mut());

    if MSG_HWND.compare_and_swap(0, hwnd as usize, Ordering::Relaxed) != 0 {
        panic!("MSG_HWND is already set - possible race condition");
    }

    if MAIN_HTHREAD.compare_and_swap(0, cur_hthread as usize, Ordering::Release) != 0 {
        panic!("MAIN_HTHREAD is already set - possible race condition");
    }

    // Now that `MAIN_HTHREAD` is initialized, we are officially in a main thread.
    debug_assert!(Wm::is_main_thread());
    let wm = unsafe { Wm::global_unchecked() };

    window::init(wm);
}

extern "system" fn msg_wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // note: This function may be called before `MSG_HWND` and `MAIN_THREAD`
    // are initialized, to handle `WM_CREATE`, etc. In such cases, calling
    // `global_unchecked` is illegal.

    match msg {
        MSG_WND_WM_INVOKE => {
            let wm = unsafe { Wm::global_unchecked() };

            let payload = unsafe { InvokePayload::from_raw(wparam as _) };
            payload(wm);
            0
        }
        WM_TIMER => {
            let wm = unsafe { Wm::global_unchecked() };

            let timer_id = wparam as UINT_PTR;

            let mut timers = TIMERS.get_with_wm(wm).borrow_mut();
            let ptr = PoolPtr(std::num::NonZeroUsize::new(timer_id).unwrap());
            let timer = if let Some(timer) = timers.deallocate(ptr) {
                timer
            } else {
                // The timer is already killed
                return 0;
            };
            drop(timers);

            // Kill the timer (Win32 timers are periodic)
            unsafe {
                KillTimer(hwnd, timer_id);
            }

            (timer.handler)(wm);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
