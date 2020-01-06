use std::{cell::Cell, ptr::null_mut, rc::Rc};
use wchar::wch_c;
use winapi::{
    shared::{
        minwindef::{LPARAM, LRESULT, UINT, WPARAM},
        windef::HWND,
    },
    um::{
        libloaderapi::GetModuleHandleW,
        winuser::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, RegisterClassW,
            SetWindowLongPtrW, ShowWindow, CW_USEDEFAULT, GWLP_USERDATA, SW_HIDE, SW_SHOW,
            WM_CREATE, WM_DESTROY, WNDCLASSW, WS_OVERLAPPED,
        },
    },
};

use super::{Wm, WndAttrs};

const WND_CLASS: &[u16] = wch_c!("TcwAppWnd");

#[derive(Debug, Clone)]
pub struct HWnd {
    wnd: Rc<Wnd>,
}

impl PartialEq for HWnd {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.wnd, &other.wnd)
    }
}

impl Eq for HWnd {}

impl std::hash::Hash for HWnd {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (&*self.wnd as *const Wnd).hash(state);
    }
}

#[derive(Debug)]
struct Wnd {
    hwnd: Cell<HWND>,
}

impl HWnd {
    fn expect_hwnd(&self) -> HWND {
        let hwnd = self.wnd.hwnd.get();
        assert!(!hwnd.is_null(), "already destroyed");
        hwnd
    }
}

/// Perform a one-time initialization for this module.
///
/// (`mt_lazy_static!` would be a better choice for module decoupling, but
/// I think that in this case, code size and runtime performance outweigh that.)
pub(super) fn init(_: Wm) {
    let hinstance = unsafe { GetModuleHandleW(null_mut()) };

    // Create a window class for the message-only window
    let wnd_class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance,
        lpszClassName: WND_CLASS.as_ptr(),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null_mut(),
    };

    unsafe { RegisterClassW(&wnd_class) };
}

pub fn new_wnd(wm: Wm, attrs: WndAttrs<'_>) -> HWnd {
    let hinstance = unsafe { GetModuleHandleW(null_mut()) };

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            WND_CLASS.as_ptr(),
            null_mut(),    // title
            WS_OVERLAPPED, // window style
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

    let pal_hwnd = HWnd {
        wnd: Rc::new(Wnd {
            hwnd: Cell::new(hwnd),
        }),
    };

    // Store `Rc<Wnd>` to `hwnd[GWLP_USERDATA]`
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Rc::into_raw(Rc::clone(&pal_hwnd.wnd)) as isize,
        );
    }

    set_wnd_attr(wm, &pal_hwnd, attrs);

    pal_hwnd
}

pub fn set_wnd_attr(_: Wm, pal_hwnd: &HWnd, attrs: WndAttrs<'_>) {
    let hwnd = pal_hwnd.expect_hwnd();

    log::warn!("update_wnd({:?}, {:?}): stub!", pal_hwnd, attrs);

    if let Some(visible) = attrs.visible {
        // Note: `ShowWindow` ignores the command and uses the value specified
        // by the program that launched the current application when it's
        // called for the first time. It's usually (but not always) a desired
        // behavior.
        let cmd = if visible { SW_SHOW } else { SW_HIDE };
        unsafe {
            ShowWindow(hwnd, cmd);
        }
    }
}

pub fn remove_wnd(_: Wm, pal_hwnd: &HWnd) {
    let hwnd = pal_hwnd.expect_hwnd();
    unsafe {
        DestroyWindow(hwnd);
    }
}

pub fn update_wnd(_: Wm, pal_hwnd: &HWnd) {
    log::warn!("update_wnd({:?}): stub!", pal_hwnd);
}

pub fn get_wnd_size(_: Wm, pal_hwnd: &HWnd) -> [u32; 2] {
    log::warn!("get_wnd_size({:?}): stub!", pal_hwnd);
    [100, 100]
}

pub fn get_wnd_dpi_scale(_: Wm, pal_hwnd: &HWnd) -> f32 {
    log::warn!("get_wnd_dpi_scale({:?}): stub!", pal_hwnd);
    1.0
}

pub fn request_update_ready_wnd(_: Wm, pal_hwnd: &HWnd) {
    log::warn!("request_update_ready_wnd({:?}): stub!", pal_hwnd);
}

extern "system" fn wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let wnd_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Wnd;

    // `wnd_ptr` is handled specially for the following lifecycle events
    match msg {
        WM_CREATE => {
            debug_assert!(wnd_ptr.is_null());
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        WM_DESTROY => {
            debug_assert!(!wnd_ptr.is_null());
            // Take and drop the strong reference to `Wnd`
            let wnd = unsafe { Rc::from_raw(wnd_ptr) };
            wnd.hwnd.set(null_mut());
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            drop(wnd);
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        _ => {}
    }

    if wnd_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let _wnd = unsafe { &*wnd_ptr };

    // TODO
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
