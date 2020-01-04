use std::{
    mem::MaybeUninit,
    ptr::null_mut,
    sync::atomic::{AtomicUsize, Ordering},
};
use wchar::wch_c;
use winapi::{
    shared::{
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
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW,
            PostQuitMessage, RegisterClassW, TranslateMessage, CW_USEDEFAULT, WM_USER, WNDCLASSW,
        },
    },
};

use super::Wm;
use crate::iface::Wm as WmTrait;

/// `HWND`
static MSG_HWND: AtomicUsize = AtomicUsize::new(0);

/// `HANDLE`
static MAIN_HTHREAD: AtomicUsize = AtomicUsize::new(0);

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

pub fn invoke_on_main_thread(f: Box<dyn FnOnce(Wm) + Send>) {
    // Make sure the main thread is initialized
    is_main_thread();

    let msg_hwnd = MSG_HWND.load(Ordering::Relaxed) as HWND;
    debug_assert_ne!(msg_hwnd, null_mut());

    // TODO: find a way to avoid double boxing
    let boxed: InvokePayload = Box::new(f);

    unsafe {
        PostMessageW(
            msg_hwnd,
            MSG_WND_WM_INVOKE,
            Box::into_raw(boxed) as WPARAM,
            0,
        );
    }
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
}

extern "system" fn msg_wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let wm = unsafe { Wm::global_unchecked() };

    match msg {
        MSG_WND_WM_INVOKE => {
            let payload = unsafe { InvokePayload::from_raw(wparam as _) };
            payload(wm);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
