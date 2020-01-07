use array::Array2;
use std::{cell::Cell, mem::MaybeUninit, ptr::null_mut, rc::Rc};
use wchar::wch_c;
use winapi::{
    shared::{
        minwindef::{LPARAM, LRESULT, UINT, WPARAM},
        windef::HWND,
    },
    um::{
        libloaderapi::GetModuleHandleW,
        winuser::{
            AdjustWindowRectExForDpi, CreateWindowExW, DefWindowProcW, DestroyWindow,
            GetClientRect, GetDpiForWindow, GetWindowLongPtrW, GetWindowLongW, RegisterClassW,
            SetWindowLongPtrW, SetWindowPos, ShowWindow, CW_USEDEFAULT, GWLP_USERDATA, GWL_EXSTYLE,
            GWL_STYLE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SW_HIDE, SW_SHOW, WM_CREATE,
            WM_DESTROY, WNDCLASSW, WS_OVERLAPPED,
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

    // TODO: min_size: Option<[u32; 2]>,
    // TODO: max_size: Option<[u32; 2]>,
    // TODO: flags: Option<WndFlags>,
    // TODO: caption: Option<Cow<'a, str>>,
    // TODO: visible: Option<bool>,
    // TODO: listener: Option<Box<dyn WndListener<T>>>,
    // TODO: layer: Option<Option<TLayer>>,
    // TODO: cursor_shape: Option<CursorShape>,

    if let Some(new_size) = attrs.size {
        let dpi = unsafe { GetDpiForWindow(hwnd) } as u32;
        assert_ne!(dpi, 0);

        // Get the current client region
        let mut rect = MaybeUninit::uninit();
        assert_ne!(unsafe { GetClientRect(hwnd, rect.as_mut_ptr()) }, 0);
        let mut rect = unsafe { rect.assume_init() };

        let size = [
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        ];
        let size = size.map(|i| phys_to_log(i, dpi));

        // Resize the window only if the logical size differs
        if size != new_size {
            if size[0] != new_size[0] {
                rect.right = rect.left + log_to_phys(new_size[0], dpi) as i32;
            }
            if size[1] != new_size[1] {
                rect.bottom = rect.top + log_to_phys(new_size[1], dpi) as i32;
            }

            // Calculate the outer size
            unsafe {
                let style = GetWindowLongW(hwnd, GWL_STYLE) as _;
                let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as _;

                assert_ne!(
                    AdjustWindowRectExForDpi(
                        &mut rect, style, 0, // the window doesn't have a menu
                        exstyle, dpi,
                    ),
                    0
                );
            }

            // Resize the window
            unsafe {
                assert_ne!(
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        0, // ignored
                        0, // ignored
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER | SWP_NOMOVE | SWP_NOACTIVATE,
                    ),
                    0
                );
            }
        }
    }

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
    let hwnd = pal_hwnd.expect_hwnd();

    // Get the size of the client region
    let mut rect = MaybeUninit::uninit();
    assert_ne!(unsafe { GetClientRect(hwnd, rect.as_mut_ptr()) }, 0);
    let rect = unsafe { rect.assume_init() };

    let size = [
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ];

    // Get the per-window DPI
    // (`GetDpiForWindow` requires Win 10, v1607)
    let dpi = unsafe { GetDpiForWindow(hwnd) } as u32;
    assert_ne!(dpi, 0);

    // Apply DPI scaling
    size.map(|i| phys_to_log(i, dpi))
}

pub fn get_wnd_dpi_scale(_: Wm, pal_hwnd: &HWnd) -> f32 {
    let hwnd = pal_hwnd.expect_hwnd();

    let dpi = unsafe { GetDpiForWindow(hwnd) };
    assert_ne!(dpi, 0);

    (dpi as f32) / 96.0
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

fn phys_to_log(x: u32, dpi: u32) -> u32 {
    (x * 96 + dpi / 2) / dpi
}

fn log_to_phys(x: u32, dpi: u32) -> u32 {
    (x * dpi + 48) / 96
}
