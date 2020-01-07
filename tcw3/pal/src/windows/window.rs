use array::Array2;
use std::{
    cell::{Cell, RefCell},
    fmt,
    mem::MaybeUninit,
    ptr::null_mut,
    rc::Rc,
};
use wchar::wch_c;
use winapi::{
    shared::{
        minwindef::{DWORD, LPARAM, LRESULT, UINT, WPARAM},
        windef::{HCURSOR, HWND},
    },
    um::{libloaderapi, winuser},
};

use super::{codecvt::str_to_c_wstr, Wm, WndAttrs};
use crate::iface;

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

struct Wnd {
    hwnd: Cell<HWND>,
    // TODO: Raise the following events:
    // - close_requested
    // - update_ready
    // - resize
    // - dpi_scale_changed
    // - mouse_motion
    // - mouse_leave
    // - mouse_drag
    // - scroll_motion
    // - scroll_gesture
    listener: RefCell<Rc<dyn iface::WndListener<Wm>>>,
    cursor: Cell<HCURSOR>,
}

impl fmt::Debug for Wnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wnd")
            .field("hwnd", &self.hwnd)
            .field("listener", &self.listener.as_ptr())
            .field("cursor", &self.cursor)
            .finish()
    }
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
    let hinstance = unsafe { libloaderapi::GetModuleHandleW(null_mut()) };

    // Create a window class for the message-only window
    let wnd_class = winuser::WNDCLASSW {
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

    unsafe { winuser::RegisterClassW(&wnd_class) };
}

pub fn new_wnd(wm: Wm, attrs: WndAttrs<'_>) -> HWnd {
    let hinstance = unsafe { libloaderapi::GetModuleHandleW(null_mut()) };

    let hwnd = unsafe {
        winuser::CreateWindowExW(
            0,
            WND_CLASS.as_ptr(),
            null_mut(), // title
            style_for_flags(Default::default()),
            winuser::CW_USEDEFAULT,
            winuser::CW_USEDEFAULT,
            winuser::CW_USEDEFAULT,
            winuser::CW_USEDEFAULT,
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
            listener: RefCell::new(Rc::new(())),
            cursor: Cell::new(unsafe { winuser::LoadCursorW(null_mut(), winuser::IDC_ARROW) }),
        }),
    };

    // Store `Rc<Wnd>` to `hwnd[winuser::GWLP_USERDATA]`
    unsafe {
        winuser::SetWindowLongPtrW(
            hwnd,
            winuser::GWLP_USERDATA,
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
    // TODO: layer: Option<Option<TLayer>>,

    if let Some(shape) = attrs.cursor_shape {
        use self::iface::CursorShape;
        let id = match shape {
            CursorShape::Arrow | CursorShape::Default => winuser::IDC_ARROW,
            CursorShape::Hand => winuser::IDC_HAND,
            CursorShape::Crosshair => winuser::IDC_CROSS,
            CursorShape::Text | CursorShape::VerticalText => winuser::IDC_IBEAM,
            CursorShape::NotAllowed | CursorShape::NoDrop => winuser::IDC_NO,
            CursorShape::Grab
            | CursorShape::Grabbing
            | CursorShape::Move
            | CursorShape::AllScroll => winuser::IDC_SIZEALL,
            CursorShape::EResize
            | CursorShape::WResize
            | CursorShape::EwResize
            | CursorShape::ColResize => winuser::IDC_SIZEWE,
            CursorShape::NResize
            | CursorShape::SResize
            | CursorShape::NsResize
            | CursorShape::RowResize => winuser::IDC_SIZENS,
            CursorShape::NeResize | CursorShape::SwResize | CursorShape::NeswResize => {
                winuser::IDC_SIZENESW
            }
            CursorShape::NwResize | CursorShape::SeResize | CursorShape::NwseResize => {
                winuser::IDC_SIZENWSE
            }
            CursorShape::Wait => winuser::IDC_WAIT,
            CursorShape::Progress => winuser::IDC_APPSTARTING,
            CursorShape::Help => winuser::IDC_HELP,
            _ => winuser::IDC_ARROW,
        };

        // TODO: Call `SetCursor` if the pointer is inside the window
        pal_hwnd
            .wnd
            .cursor
            .set(unsafe { winuser::LoadCursorW(null_mut(), id) });
    }

    if let Some(flags) = attrs.flags {
        let style = unsafe { winuser::GetWindowLongW(hwnd, winuser::GWL_STYLE) } as DWORD;

        let new_style = style
            & !(winuser::WS_CHILD
                | winuser::WS_OVERLAPPED
                | winuser::WS_CAPTION
                | winuser::WS_SYSMENU
                | winuser::WS_THICKFRAME
                | winuser::WS_MINIMIZEBOX
                | winuser::WS_MAXIMIZEBOX)
            | style_for_flags(flags);

        unsafe {
            winuser::SetWindowLongW(hwnd, winuser::GWL_STYLE, new_style as _);
        }
    }

    if let Some(new_size) = attrs.size {
        let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
        assert_ne!(dpi, 0);

        // Get the current client region
        let mut rect = MaybeUninit::uninit();
        assert_ne!(
            unsafe { winuser::GetClientRect(hwnd, rect.as_mut_ptr()) },
            0
        );
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
                let style = winuser::GetWindowLongW(hwnd, winuser::GWL_STYLE) as _;
                let exstyle = winuser::GetWindowLongW(hwnd, winuser::GWL_EXSTYLE) as _;

                assert_ne!(
                    winuser::AdjustWindowRectExForDpi(
                        &mut rect, style, 0, // the window doesn't have a menu
                        exstyle, dpi,
                    ),
                    0
                );
            }

            // Resize the window
            unsafe {
                assert_ne!(
                    winuser::SetWindowPos(
                        hwnd,
                        null_mut(),
                        0, // ignored
                        0, // ignored
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        winuser::SWP_NOZORDER | winuser::SWP_NOMOVE | winuser::SWP_NOACTIVATE,
                    ),
                    0
                );
            }
        }
    }

    if let Some(caption) = attrs.caption {
        let caption_w = str_to_c_wstr(&caption);
        unsafe {
            winuser::SetWindowTextW(hwnd, caption_w.as_ptr());
        }
    }

    if let Some(listener) = attrs.listener {
        pal_hwnd.wnd.listener.replace(Rc::from(listener));
    }

    if let Some(visible) = attrs.visible {
        // Note: `ShowWindow` ignores the command and uses the value specified
        // by the program that launched the current application when it's
        // called for the first time. It's usually (but not always) a desired
        // behavior.
        let cmd = if visible {
            winuser::SW_SHOW
        } else {
            winuser::SW_HIDE
        };
        unsafe {
            winuser::ShowWindow(hwnd, cmd);
        }
    }
}

fn style_for_flags(flags: iface::WndFlags) -> DWORD {
    use iface::WndFlags;
    let mut out = if flags.contains(WndFlags::BORDERLESS) {
        winuser::WS_CHILD
    } else {
        winuser::WS_OVERLAPPED | winuser::WS_CAPTION | winuser::WS_SYSMENU
    };

    if flags.contains(WndFlags::RESIZABLE) {
        out |= winuser::WS_THICKFRAME | winuser::WS_MINIMIZEBOX | winuser::WS_MAXIMIZEBOX;
    }

    out
}

pub fn remove_wnd(_: Wm, pal_hwnd: &HWnd) {
    let hwnd = pal_hwnd.expect_hwnd();
    unsafe {
        winuser::DestroyWindow(hwnd);
    }
}

pub fn update_wnd(_: Wm, pal_hwnd: &HWnd) {
    log::warn!("update_wnd({:?}): stub!", pal_hwnd);
}

pub fn get_wnd_size(_: Wm, pal_hwnd: &HWnd) -> [u32; 2] {
    let hwnd = pal_hwnd.expect_hwnd();

    // Get the size of the client region
    let mut rect = MaybeUninit::uninit();
    assert_ne!(
        unsafe { winuser::GetClientRect(hwnd, rect.as_mut_ptr()) },
        0
    );
    let rect = unsafe { rect.assume_init() };

    let size = [
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ];

    // Get the per-window DPI
    // (`GetDpiForWindow` requires Win 10, v1607)
    let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
    assert_ne!(dpi, 0);

    // Apply DPI scaling
    size.map(|i| phys_to_log(i, dpi))
}

pub fn get_wnd_dpi_scale(_: Wm, pal_hwnd: &HWnd) -> f32 {
    let hwnd = pal_hwnd.expect_hwnd();

    let dpi = unsafe { winuser::GetDpiForWindow(hwnd) };
    assert_ne!(dpi, 0);

    (dpi as f32) / 96.0
}

pub fn request_update_ready_wnd(_: Wm, pal_hwnd: &HWnd) {
    log::warn!("request_update_ready_wnd({:?}): stub!", pal_hwnd);
}

extern "system" fn wnd_proc(hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let wnd_ptr = unsafe { winuser::GetWindowLongPtrW(hwnd, winuser::GWLP_USERDATA) } as *const Wnd;

    // `wnd_ptr` is handled specially for the following lifecycle events
    match msg {
        winuser::WM_CREATE => {
            debug_assert!(wnd_ptr.is_null());
            return unsafe { winuser::DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        winuser::WM_DESTROY => {
            debug_assert!(!wnd_ptr.is_null());
            // Take and drop the strong reference to `Wnd`
            let wnd = unsafe { Rc::from_raw(wnd_ptr) };
            wnd.hwnd.set(null_mut());
            unsafe {
                winuser::SetWindowLongPtrW(hwnd, winuser::GWLP_USERDATA, 0);
            }
            drop(wnd);
            return unsafe { winuser::DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        _ => {}
    }

    if wnd_ptr.is_null() {
        return unsafe { winuser::DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // Clone `Rc<Wnd>` from `winuser::GWLP_USERDATA`
    let wnd = unsafe { Rc::from_raw(wnd_ptr) };
    std::mem::forget(Rc::clone(&wnd));

    match msg {
        winuser::WM_SETCURSOR => {
            if lparam & 0xffff == winuser::HTCLIENT {
                unsafe {
                    winuser::SetCursor(wnd.cursor.get());
                }
                return 1;
            }
        }
        _ => {}
    }

    drop(wnd);
    unsafe { winuser::DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn phys_to_log(x: u32, dpi: u32) -> u32 {
    // Must be rounded up so that the drawn region (which is sized according to
    // the logical size because the user only knows the logical size) completely
    // covers a window's client region.
    (x * 96 + dpi - 1) / dpi
}

fn log_to_phys(x: u32, dpi: u32) -> u32 {
    // Must be rounded down so that `phys_to_log . log_to_phys` is an identity
    // operation when `dpi >= 96`.
    x * dpi / 96
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn phys_log_roundtrip(x: u16, dpi: u8) -> bool {
        let x = x as u32;
        let dpi = dpi as u32 + 96; // assume `dpi >= 96`
        phys_to_log(log_to_phys(x, dpi), dpi) == x
    }
}
