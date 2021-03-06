use array::Array2;
use log::trace;
use std::{
    cell::{Cell, RefCell},
    convert::TryInto,
    fmt,
    mem::{size_of, MaybeUninit},
    ptr::null_mut,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
use wchar::wch_c;
use winapi::{
    shared::{
        minwindef::{DWORD, HIWORD, LOWORD, LPARAM, LRESULT, UINT, WPARAM},
        ntdef::LONG,
        windef::{HCURSOR, HICON, HWND, POINT, RECT, SIZE},
    },
    um::{dwmapi, libloaderapi, uxtheme, winuser},
};

use super::{
    acceltable,
    codecvt::str_to_c_wstr,
    comp, frameclock,
    textinput::TextInputWindow,
    utils::{assert_win32_nonnull, assert_win32_ok},
    AccelTable, Wm, WndAttrs,
};
use crate::{iface, prelude::*};

const WND_CLASS: &[u16] = wch_c!("TcwAppWnd");

/// Mouse buttons
mod buttons {
    pub const L: u8 = 0;
    pub const R: u8 = 1;
    pub const M: u8 = 2;
    pub const X1: u8 = 3;
    pub const X2: u8 = 4;
}

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
    listener: RefCell<Rc<dyn iface::WndListener<Wm>>>,
    cursor: Cell<HCURSOR>,
    comp_wnd: comp::CompWnd,
    min_size: Cell<[u32; 2]>,
    max_size: Cell<[u32; 2]>,
    flags: Cell<iface::WndFlags>,
    /// Used by `FrameClockManager` through the trait `FrameClockClient`
    update_ready_pending: Cell<bool>,

    drag_state: RefCell<Option<MouseDragState>>,

    text_input_wnd: TextInputWindow,
}

impl fmt::Debug for Wnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wnd")
            .field("hwnd", &self.hwnd)
            .field("listener", &self.listener.as_ptr())
            .field("cursor", &self.cursor)
            .field("comp_wnd", &self.comp_wnd)
            .field("min_size", &self.min_size)
            .field("max_size", &self.max_size)
            .field("flags", &self.flags)
            .finish()
    }
}

struct MouseDragState {
    listener: Rc<dyn iface::MouseDragListener<Wm>>,
    pressed_buttons: u8,
}

/// Hard-coded limit for window size for various calculations not to overflow
const MAX_WND_SIZE: u32 = 0x10000;

impl HWnd {
    pub(super) fn expect_hwnd(&self) -> HWND {
        let hwnd = self.wnd.hwnd.get();
        assert!(!hwnd.is_null(), "already destroyed");
        hwnd
    }

    pub(super) fn text_input_wnd(&self) -> &TextInputWindow {
        &self.wnd.text_input_wnd
    }
}

static APP_HICON: AtomicUsize = AtomicUsize::new(0);

/// Set the icon used in application windows.
///
/// This function should be called before the main thread is initialized
/// to be effective.
pub unsafe fn set_app_hicon(hicon: HICON) {
    APP_HICON.store(hicon as _, Ordering::Relaxed);
}

/// Perform a one-time initialization for this module.
///
/// (`mt_lazy_static!` would be a better choice for module decoupling, but
/// I think that in this case, code size and runtime performance outweigh that.)
pub(super) fn init(_: Wm) {
    let hinstance = unsafe { libloaderapi::GetModuleHandleW(null_mut()) };

    // Create a window class for application windows
    let wnd_class = winuser::WNDCLASSW {
        style: winuser::CS_HREDRAW | winuser::CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance,
        lpszClassName: WND_CLASS.as_ptr(),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hIcon: APP_HICON.load(Ordering::Relaxed) as _,
        hCursor: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null_mut(),
    };

    unsafe { winuser::RegisterClassW(&wnd_class) };
}

pub fn new_wnd(wm: Wm, attrs: WndAttrs<'_>) -> HWnd {
    let hinstance = unsafe { libloaderapi::GetModuleHandleW(null_mut()) };

    let hwnd = assert_win32_nonnull(unsafe {
        winuser::CreateWindowExW(
            winuser::WS_EX_NOREDIRECTIONBITMAP,
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
    });

    let comp_wnd = comp::CompWnd::new(wm, hwnd);

    let pal_hwnd = HWnd {
        wnd: Rc::new(Wnd {
            hwnd: Cell::new(hwnd),
            listener: RefCell::new(Rc::new(())),
            cursor: Cell::new(unsafe { winuser::LoadCursorW(null_mut(), winuser::IDC_ARROW) }),
            comp_wnd,
            min_size: Cell::new([0; 2]),
            max_size: Cell::new([MAX_WND_SIZE; 2]),
            flags: Cell::new(iface::WndFlags::default()),
            update_ready_pending: Cell::new(false),
            drag_state: RefCell::new(None),
            text_input_wnd: TextInputWindow::new(),
        }),
    };

    // Store `Rc<Wnd>` to `hwnd[winuser::GWLP_USERDATA]`
    unsafe {
        winuser::SetWindowLongPtrW(
            hwnd,
            winuser::GWLP_USERDATA,
            Rc::into_raw(Rc::clone(&pal_hwnd.wnd)) as _,
        );
    }

    set_wnd_attr(wm, &pal_hwnd, attrs);

    pal_hwnd
}

pub fn set_wnd_attr(_: Wm, pal_hwnd: &HWnd, attrs: WndAttrs<'_>) {
    let hwnd = pal_hwnd.expect_hwnd();

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

        let cursor = unsafe { winuser::LoadCursorW(null_mut(), id) };
        pal_hwnd.wnd.cursor.set(cursor);

        if is_mouse_in_wnd(hwnd) {
            unsafe {
                winuser::SetCursor(cursor);
            }
        }
    }

    if let Some(flags) = attrs.flags {
        let diff = flags ^ pal_hwnd.wnd.flags.get();
        pal_hwnd.wnd.flags.set(flags);

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

        if diff.contains(iface::WndFlags::FULL_SIZE_CONTENT) {
            update_wnd_frame(pal_hwnd);
        }
    }

    use std::cmp::min;
    if let Some(new_size) = attrs.min_size {
        // Clamp the value to a sane range for the calculation not to overflow
        pal_hwnd
            .wnd
            .min_size
            .set(new_size.map(|i| min(i, MAX_WND_SIZE)));
    }
    if let Some(new_size) = attrs.max_size {
        // Ditto.
        pal_hwnd
            .wnd
            .max_size
            .set(new_size.map(|i| min(i, MAX_WND_SIZE)));
    }

    if let Some(new_size) = attrs.size {
        let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
        assert_win32_ok(dpi);

        // Get the current client region
        let mut rect = MaybeUninit::uninit();
        assert_win32_ok(unsafe { winuser::GetClientRect(hwnd, rect.as_mut_ptr()) });
        let mut rect = unsafe { rect.assume_init() };

        let size = [
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        ];
        let size = size.map(|i| phy_to_log(i, dpi));

        // Resize the window only if the logical size differs
        // (That's why we don't use `log_inner_to_phy_outer` here)
        if size != new_size {
            if size[0] != new_size[0] {
                rect.right = rect.left + log_to_phy(new_size[0], dpi) as i32;
            }
            if size[1] != new_size[1] {
                rect.bottom = rect.top + log_to_phy(new_size[1], dpi) as i32;
            }

            // Calculate the outer size
            if !pal_hwnd
                .wnd
                .flags
                .get()
                .contains(iface::WndFlags::FULL_SIZE_CONTENT)
            {
                unsafe {
                    let style = winuser::GetWindowLongW(hwnd, winuser::GWL_STYLE) as _;
                    let exstyle = winuser::GetWindowLongW(hwnd, winuser::GWL_EXSTYLE) as _;

                    assert_win32_ok(winuser::AdjustWindowRectExForDpi(
                        &mut rect, style, 0, // the window doesn't have a menu
                        exstyle, dpi,
                    ));
                }
            }

            // Resize the window
            unsafe {
                assert_win32_ok(winuser::SetWindowPos(
                    hwnd,
                    null_mut(),
                    0, // ignored
                    0, // ignored
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    winuser::SWP_NOZORDER
                        | winuser::SWP_NOMOVE
                        | winuser::SWP_NOACTIVATE
                        | winuser::SWP_NOOWNERZORDER,
                ));
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

    if let Some(layer) = attrs.layer {
        pal_hwnd.wnd.comp_wnd.set_layer(hwnd, layer);
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
    } | winuser::WS_CLIPSIBLINGS;

    if flags.contains(WndFlags::RESIZABLE) {
        out |= winuser::WS_THICKFRAME | winuser::WS_MINIMIZEBOX | winuser::WS_MAXIMIZEBOX;
    }

    out
}

fn is_mouse_in_wnd(hwnd: HWND) -> bool {
    // Our window enables mouse tracking with the `TME_LEAVE` flag whenever
    // the mouse pointer enters. The flag is automatically cleared by the
    // system when the mouse pointer leaves the window.
    //
    // `TrackMouseEvent` also lets us query the current state, so we can use
    // it to check if the mouse pointer is inside the window.
    let mut te = winuser::TRACKMOUSEEVENT {
        cbSize: size_of::<winuser::TRACKMOUSEEVENT>() as u32,
        dwFlags: winuser::TME_QUERY,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };

    unsafe {
        assert_win32_ok(winuser::TrackMouseEvent(&mut te));
    }

    te.dwFlags & winuser::TME_LEAVE != 0
}

pub fn remove_wnd(wm: Wm, pal_hwnd: &HWnd) {
    // Invalidate all text input contexts associated with the window
    pal_hwnd.wnd.text_input_wnd.invalidate(wm);

    let hwnd = pal_hwnd.expect_hwnd();
    unsafe {
        winuser::DestroyWindow(hwnd);
    }
}

pub fn update_wnd(_: Wm, _pal_hwnd: &HWnd) {
    // Composition is implicitly flushed, so nothing to do here
}

pub fn get_wnd_size(_: Wm, pal_hwnd: &HWnd) -> [u32; 2] {
    let hwnd = pal_hwnd.expect_hwnd();

    // Get the size of the client region
    let mut rect = MaybeUninit::uninit();
    assert_win32_ok(unsafe { winuser::GetClientRect(hwnd, rect.as_mut_ptr()) });
    let rect = unsafe { rect.assume_init() };

    let size = [
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ];

    // Get the per-window DPI
    // (`GetDpiForWindow` requires Win 10, v1607)
    let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
    assert_win32_ok(dpi);

    // Apply DPI scaling
    size.map(|i| phy_to_log(i, dpi))
}

pub fn get_wnd_dpi_scale(_: Wm, pal_hwnd: &HWnd) -> f32 {
    let hwnd = pal_hwnd.expect_hwnd();

    let dpi = unsafe { winuser::GetDpiForWindow(hwnd) };
    assert_win32_ok(dpi);

    (dpi as f32) / 96.0
}

pub fn is_wnd_focused(_: Wm, pal_hwnd: &HWnd) -> bool {
    let hwnd = pal_hwnd.expect_hwnd();

    hwnd == unsafe { winuser::GetForegroundWindow() }
}

static FRAME_CLOCK_MANAGER: frameclock::FrameClockManager<HWnd> =
    frameclock::FrameClockManager::new();

impl frameclock::FrameClockClient for HWnd {
    fn set_pending(&mut self, x: bool) {
        self.wnd.update_ready_pending.set(x);
    }
    fn is_pending(&mut self) -> bool {
        self.wnd.update_ready_pending.get()
    }
    fn handle_frame_clock(&mut self, wm: Wm) {
        if self.wnd.hwnd.get().is_null() {
            // already deleted
            return;
        }

        let listener = Rc::clone(&self.wnd.listener.borrow());
        listener.update_ready(wm, &self);
    }
}

pub fn request_update_ready_wnd(wm: Wm, pal_hwnd: &HWnd) {
    FRAME_CLOCK_MANAGER.register(wm, pal_hwnd.clone());
}

fn update_wnd_frame(pal_hwnd: &HWnd) {
    adjust_dwm_frame(pal_hwnd);

    unsafe {
        assert_win32_ok(winuser::SetWindowPos(
            pal_hwnd.expect_hwnd(),
            null_mut(),
            0,
            0,
            0,
            0,
            winuser::SWP_NOZORDER
                | winuser::SWP_NOMOVE
                | winuser::SWP_NOSIZE
                | winuser::SWP_NOACTIVATE
                | winuser::SWP_NOOWNERZORDER
                | winuser::SWP_FRAMECHANGED,
        ));
        assert_win32_ok(winuser::PostMessageW(
            pal_hwnd.expect_hwnd(),
            winuser::WM_SIZE,
            0,
            0,
        ));
    }
}

fn adjust_dwm_frame(pal_hwnd: &HWnd) {
    let hwnd = pal_hwnd.expect_hwnd();

    let margins = if pal_hwnd
        .wnd
        .flags
        .get()
        .contains(iface::WndFlags::FULL_SIZE_CONTENT)
    {
        // The margins must be at least 1 pixel for the shadow to appear
        uxtheme::MARGINS {
            cxLeftWidth: 1,
            cxRightWidth: 1,
            cyBottomHeight: 1,
            cyTopHeight: 1,
        }
    } else {
        uxtheme::MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyBottomHeight: 0,
            cyTopHeight: 0,
        }
    };
    unsafe {
        dwmapi::DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

struct EnumAccel<F: FnMut(&AccelTable)>(F);

impl<F: FnMut(&AccelTable)> iface::InterpretEventCtx<AccelTable> for EnumAccel<F> {
    fn use_accel(&mut self, accel: &AccelTable) {
        (self.0)(accel);
    }
}

struct KeyEvent {
    key: u16,
    mod_flags: u8,
}

impl iface::KeyEvent<AccelTable> for KeyEvent {
    fn translate_accel(&self, accel_table: &AccelTable) -> Option<iface::ActionId> {
        accel_table.find_action_with_key(self.key, self.mod_flags)
    }
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

    let wm = unsafe { Wm::global_unchecked() };
    let pal_hwnd = HWnd { wnd };

    match msg {
        winuser::WM_ACTIVATE => {
            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.focus(wm, &pal_hwnd);

            // `DwmExtendFrameIntoClientArea` should be called every time
            // `WM_ACTIVATE` is sent
            adjust_dwm_frame(&pal_hwnd);
        } // WM_ACTIVATE

        winuser::WM_CLOSE => {
            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.close_requested(wm, &pal_hwnd);

            // Prevent the default action (destroying the window) by not
            // calling `DefWindowProc`
            return 0;
        } // WM_CLOSE

        winuser::WM_DPICHANGED => {
            // <https://docs.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged>:
            // > In order to handle this message correctly, you will need to
            // > resize and reposition your window based on the suggestions
            // > provided by lParam and using SetWindowPos.
            let rect = unsafe { &*(lparam as *mut RECT) };

            trace!(
                "Received WM_DPICHANGED (new_dpi = {:?}, suggested_rect = {:?})",
                (LOWORD(wparam as _), HIWORD(wparam as _)),
                cggeom::box2! {
                    min: [rect.left, rect.top],
                    max: [rect.right, rect.bottom]
                }
                .display_im(),
            );

            unsafe {
                assert_win32_ok(winuser::SetWindowPos(
                    hwnd,
                    null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    winuser::SWP_NOZORDER | winuser::SWP_NOACTIVATE | winuser::SWP_NOOWNERZORDER,
                ));
            }

            pal_hwnd.wnd.comp_wnd.handle_dpi_change(hwnd);

            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.dpi_scale_changed(wm, &pal_hwnd);
        } // WM_DPICHANGED

        winuser::WM_GETDPISCALEDSIZE => {
            let new_dpi = wparam as u32;
            let size_result = unsafe { &mut *(lparam as *mut SIZE) };

            // The rumor [^1] says that the system rounds off the window size
            // every time the user moves the window to a monitor with
            // a different DPI, so if the user keeps moving the window back
            // and forth, the window size will gradually deviate from the
            // original size.
            //
            // [^1]: https://8thway.blogspot.com/2014/06/wpf-per-monitor-dpi.html
            //
            // We try to mitigate this issue by remembering the logical size and
            // preserving it on DPI change.

            // Get the current logical size
            let orig_size = get_wnd_size(wm, &pal_hwnd);

            // Calculate the outer size using the new DPI
            let should_adjust_for_border =
                !(pal_hwnd.wnd.flags.get()).contains(iface::WndFlags::FULL_SIZE_CONTENT);
            let req_size =
                log_inner_to_phy_outer(hwnd, new_dpi, orig_size, should_adjust_for_border);

            trace!(
                "Received WM_GETDPISCALEDSIZE (new_dpi = {:?}, suggested_size = {:?}). Returning {:?}",
                new_dpi,
                [size_result.cx, size_result.cy],
                req_size,
            );

            // Override the system-calculated size
            size_result.cx = req_size[0];
            size_result.cy = req_size[1];
            return 1;
        } // WM_GETDPISCALEDSIZE

        winuser::WM_GETMINMAXINFO => {
            use std::cmp::{max, min};
            let mut mmi = unsafe { &mut *(lparam as *mut winuser::MINMAXINFO) };
            let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
            let should_adjust_for_border =
                !(pal_hwnd.wnd.flags.get()).contains(iface::WndFlags::FULL_SIZE_CONTENT);
            let min_size = log_inner_to_phy_outer(
                hwnd,
                dpi,
                pal_hwnd.wnd.min_size.get(),
                should_adjust_for_border,
            );
            let max_size = log_inner_to_phy_outer(
                hwnd,
                dpi,
                pal_hwnd.wnd.max_size.get(),
                should_adjust_for_border,
            );

            mmi.ptMinTrackSize.x = max(mmi.ptMinTrackSize.x, min_size[0]);
            mmi.ptMinTrackSize.y = max(mmi.ptMinTrackSize.y, min_size[1]);
            mmi.ptMaxTrackSize.x = min(mmi.ptMaxTrackSize.x, max_size[0]);
            mmi.ptMaxTrackSize.y = min(mmi.ptMaxTrackSize.y, max_size[1]);

            return 0;
        } // WM_GETMINMAXINFO

        winuser::WM_CHAR => {
            log::trace!("WM_CHAR {:?}", (wparam, lparam));
            if wparam < 32 {
                log::trace!("WM_CHAR: Ignoring a control character");
                return 0;
            }
            pal_hwnd.wnd.text_input_wnd.on_char(wm, wparam as _);
            return 0;
        } // WM_CHAR

        winuser::WM_UNICHAR => {
            log::trace!("WM_UNICHAR {:?}", (wparam, lparam));
            if wparam == winuser::UNICODE_NOCHAR {
                // We can handle `WM_UNIUSER`, so return `1`
                return 1;
            }
            pal_hwnd.wnd.text_input_wnd.on_char(wm, wparam as _);
            return 0;
        } // WM_UNICHAR

        winuser::WM_KEYDOWN => {
            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());

            log::trace!("WM_KEYDOWN(0x{:x}, 0x{:x})", wparam, lparam);

            // Check the state of the modifier keys
            let mod_flags = AccelTable::query_mod_flags();

            let mut action = None;
            let action_ref = &mut action;
            let key: u16 = if let Ok(x) = wparam.try_into() {
                x
            } else {
                log::warn!(
                    "... virtual key code is out of range ({:?}), ignoring",
                    wparam
                );
                0 // Invalid virtual key code
            };
            let mut interpret_event_ctx = EnumAccel(move |accel_table| {
                if action_ref.is_none() {
                    *action_ref = accel_table.find_action_with_key(key, mod_flags);
                }
            });

            listener.interpret_event(wm, &pal_hwnd, &mut interpret_event_ctx);

            // Interpret text input actions. Do this after calling `interpret_event`
            // so that they can be shadowed by custom accelerator tables.
            if pal_hwnd.wnd.text_input_wnd.is_active() {
                iface::InterpretEventCtx::use_accel(
                    &mut interpret_event_ctx,
                    &acceltable::TEXT_INPUT_ACCEL,
                );
            }

            log::trace!("... action = {:?}", action);

            if let Some(action) = action {
                // The action was found. Can the window handle it?
                let status = listener.validate_action(wm, &pal_hwnd, action);
                if status.contains(iface::ActionStatus::VALID) {
                    if status.contains(iface::ActionStatus::ENABLED) {
                        listener.perform_action(wm, &pal_hwnd, action);
                    }
                    return 0;
                }
            }

            let handled = listener.key_down(wm, &pal_hwnd, &KeyEvent { key, mod_flags });
            log::trace!("... key_down(...) = {:?}", handled);

            if handled {
                return 0;
            }
        } // WM_KEYDOWN

        winuser::WM_KEYUP => {
            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());

            log::trace!("WM_KEYUP(0x{:x}, 0x{:x})", wparam, lparam);

            // Check the state of the modifier keys
            let mod_flags = AccelTable::query_mod_flags();

            let key: u16 = if let Ok(x) = wparam.try_into() {
                x
            } else {
                log::warn!(
                    "... virtual key code is out of range ({:?}), ignoring",
                    wparam
                );
                0 // Invalid virtual key code
            };

            let handled = listener.key_up(wm, &pal_hwnd, &KeyEvent { key, mod_flags });
            log::trace!("... key_up(...) = {:?}", handled);

            if handled {
                return 0;
            }
        } // WM_KEYUP

        winuser::WM_SETCURSOR => {
            if lparam & 0xffff == winuser::HTCLIENT {
                unsafe {
                    winuser::SetCursor(pal_hwnd.wnd.cursor.get());
                }
                return 1;
            }
        } // WM_SETCURSOR

        winuser::WM_MOUSEMOVE => {
            let mut te = winuser::TRACKMOUSEEVENT {
                cbSize: size_of::<winuser::TRACKMOUSEEVENT>() as u32,
                dwFlags: winuser::TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };

            unsafe {
                assert_win32_ok(winuser::TrackMouseEvent(&mut te));
            }

            let loc = lparam_to_mouse_loc(hwnd, lparam, false);

            let drag_state_cell = pal_hwnd.wnd.drag_state.borrow();
            if let Some(drag_state) = &*drag_state_cell {
                let drag_listener = Rc::clone(&drag_state.listener);
                drop(drag_state_cell);

                drag_listener.mouse_motion(wm, &pal_hwnd, loc);
                return 0;
            } else {
                drop(drag_state_cell);
            }

            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.mouse_motion(wm, &pal_hwnd, loc);

            return 0;
        } // WM_MOUSEMOVE

        winuser::WM_MOUSELEAVE => {
            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.mouse_leave(wm, &pal_hwnd);
        } // WM_MOUSELEAVE

        // TODO: Use the pointer API (https://docs.microsoft.com/en-us/previous-versions/windows/desktop/inputmsg/messages-and-notifications)
        winuser::WM_LBUTTONDOWN
        | winuser::WM_RBUTTONDOWN
        | winuser::WM_MBUTTONDOWN
        | winuser::WM_XBUTTONDOWN => {
            let button = match msg {
                winuser::WM_LBUTTONDOWN => buttons::L,
                winuser::WM_RBUTTONDOWN => buttons::R,
                winuser::WM_MBUTTONDOWN => buttons::M,
                winuser::WM_XBUTTONDOWN => match HIWORD(wparam as _) {
                    1 => buttons::X1,
                    2 => buttons::X2,
                    _ => return 0,
                },
                _ => unreachable!(),
            };
            let button_mask = 1u8 << button;
            let loc = lparam_to_mouse_loc(hwnd, lparam, false);

            let mut drag_state_cell = pal_hwnd.wnd.drag_state.borrow_mut();

            let drag_state = if let Some(drag_state) = &mut *drag_state_cell {
                drag_state
            } else {
                // Unborrow `drag_state_cell` before calling into user code
                let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
                drop(drag_state_cell);

                // Create `MouseDragState`
                let drag_state = MouseDragState {
                    listener: listener.mouse_drag(wm, &pal_hwnd, loc, button).into(),
                    pressed_buttons: 0,
                };

                unsafe { winuser::SetCapture(hwnd) };

                // Re-borrow `drag_state_cell` and set `drag_state`
                drag_state_cell = pal_hwnd.wnd.drag_state.borrow_mut();
                debug_assert!(drag_state_cell.is_none());
                *drag_state_cell = Some(drag_state);
                drag_state_cell.as_mut().unwrap()
            };

            if (drag_state.pressed_buttons & button_mask) != 0 {
                return 0;
            }
            drag_state.pressed_buttons |= button_mask;

            // Call `MouseDragListener::mouse_down`
            let drag_listener = Rc::clone(&drag_state.listener);

            drop(drag_state_cell);
            drag_listener.mouse_down(wm, &pal_hwnd, loc, button);

            return 0;
        } // WM_LBUTTONDOWN | ...

        winuser::WM_LBUTTONUP
        | winuser::WM_RBUTTONUP
        | winuser::WM_MBUTTONUP
        | winuser::WM_XBUTTONUP => {
            let button = match msg {
                winuser::WM_LBUTTONUP => buttons::L,
                winuser::WM_RBUTTONUP => buttons::R,
                winuser::WM_MBUTTONUP => buttons::M,
                winuser::WM_XBUTTONUP => match HIWORD(wparam as _) {
                    1 => buttons::X1,
                    2 => buttons::X2,
                    _ => return 0,
                },
                _ => unreachable!(),
            };
            let button_mask = 1u8 << button;
            let loc = lparam_to_mouse_loc(hwnd, lparam, false);

            let mut drag_state_cell = pal_hwnd.wnd.drag_state.borrow_mut();
            let drag_state = if let Some(drag_state) = &mut *drag_state_cell {
                drag_state
            } else {
                return 0;
            };

            if (drag_state.pressed_buttons & button_mask) == 0 {
                return 0;
            }
            drag_state.pressed_buttons &= !button_mask;

            let (drag_listener, release) = if drag_state.pressed_buttons == 0 {
                // Remove `MouseDragState` from `Wnd`
                (drag_state_cell.take().unwrap().listener, true)
            } else {
                (Rc::clone(&drag_state.listener), false)
            };

            // Call `MouseDragListener::mouse_up`
            drop(drag_state_cell);
            drag_listener.mouse_up(wm, &pal_hwnd, loc, button);

            // `ReleaseCapture` will generate a `WM_CAPTURECHANGED` message, so
            // it should be called last
            if release {
                unsafe { winuser::ReleaseCapture() };
            }
        } // WM_LBUTTONUP | ...

        winuser::WM_CAPTURECHANGED => {
            if let Some(drag_state) = pal_hwnd.wnd.drag_state.borrow_mut().take() {
                drag_state.listener.cancel(wm, &pal_hwnd);
            }
        } // WM_CAPTURECHANGED

        // TODO: Generate continuous scroll events by using the Direct Manipulation APIs
        //       (https://docs.microsoft.com/en-us/previous-versions/windows/desktop/directmanipulation/direct-manipulation-portal)
        winuser::WM_MOUSEWHEEL | winuser::WM_MOUSEHWHEEL => {
            let loc = lparam_to_mouse_loc(hwnd, lparam, true);
            let axis = (msg == winuser::WM_MOUSEWHEEL) as usize;

            // Convert the value to `ScrollDelta`
            let mut amount = winuser::GET_WHEEL_DELTA_WPARAM(wparam) as f32 / [-120.0, 120.0][axis];

            amount *= unsafe {
                let mut out = MaybeUninit::<UINT>::uninit();
                assert_win32_ok(winuser::SystemParametersInfoW(
                    [
                        winuser::SPI_GETWHEELSCROLLCHARS,
                        winuser::SPI_GETWHEELSCROLLLINES,
                    ][axis],
                    0,
                    out.as_mut_ptr() as _,
                    0,
                ));
                out.assume_init() as f32
            };

            let mut delta = iface::ScrollDelta {
                precise: false,
                delta: [0.0; 2].into(),
            };
            delta.delta[axis] = amount;

            // Call the handler
            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.scroll_motion(wm, &pal_hwnd, loc, &delta);

            return 0;
        } // WM_MOUSEWHEEL

        winuser::WM_NCHITTEST => {
            let mut hit = if (pal_hwnd.wnd.flags.get()).contains(iface::WndFlags::FULL_SIZE_CONTENT)
            {
                // When we remove the window frame, we have to do non-client hit
                // testing by ourselves.
                let lparam = lparam as DWORD;
                let loc = [
                    LOWORD(lparam) as i16 as LONG, // `GET_X_LPARAM(lparam) as LONG`
                    HIWORD(lparam) as i16 as LONG, // `GET_Y_LPARAM(lparam) as LONG`
                ];

                let rect = {
                    let mut rect = MaybeUninit::uninit();
                    assert_win32_ok(unsafe { winuser::GetWindowRect(hwnd, rect.as_mut_ptr()) });
                    unsafe { rect.assume_init() }
                };

                let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
                assert_win32_ok(dpi);

                // The idomatic solution would be to use `GetSystemMetricsForDpi`, but
                // if the user specifies a really large sizing border, it might break
                // the application's functionality. Thus, we hard-code the width for
                // now.
                let width = log_to_phy(5, dpi) as LONG;

                let flags = 0b0001 * (loc[0] < rect.left + width) as u8
                    | 0b0010 * (loc[1] < rect.top + width) as u8
                    | 0b0100 * (loc[0] >= rect.right - width) as u8
                    | 0b1000 * (loc[1] >= rect.bottom - width) as u8;

                match flags {
                    0b0000 => winuser::HTCLIENT,
                    0b0001 => winuser::HTLEFT,
                    0b0010 => winuser::HTTOP,
                    0b0011 => winuser::HTTOPLEFT,
                    0b0100 => winuser::HTRIGHT,
                    0b0101 => winuser::HTRIGHT, // invalid
                    0b0110 => winuser::HTTOPRIGHT,
                    0b0111 => winuser::HTTOPRIGHT, // invalid
                    0b1000 => winuser::HTBOTTOM,
                    0b1001 => winuser::HTBOTTOMLEFT,
                    0b1010 => winuser::HTTOP,     // invalid
                    0b1011 => winuser::HTTOPLEFT, // invalid
                    0b1100 => winuser::HTBOTTOMRIGHT,
                    0b1101 => winuser::HTBOTTOMRIGHT, // invalid
                    0b1110 => winuser::HTTOPRIGHT,    // invalid
                    0b1111 => winuser::HTTOPRIGHT,    // invalid
                    _ => unreachable!(),
                }
            } else {
                unsafe { winuser::DefWindowProcW(hwnd, msg, wparam, lparam) as _ }
            };

            if hit == winuser::HTCLIENT {
                let loc = lparam_to_mouse_loc(hwnd, lparam, true);
                let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
                match listener.nc_hit_test(wm, &pal_hwnd, loc) {
                    iface::NcHit::Client => {}
                    iface::NcHit::Grab => {
                        hit = winuser::HTCAPTION;
                    }
                };
            }

            return hit as _;
        }

        winuser::WM_NCCALCSIZE => {
            if wparam != 0
                && (pal_hwnd.wnd.flags.get()).contains(iface::WndFlags::FULL_SIZE_CONTENT)
            {
                let ncsp = unsafe { &mut *(lparam as *mut winuser::NCCALCSIZE_PARAMS) };
                // Omit the call to `DefWindowProcW` to remove the default
                // frame (including the title bar).

                // Adjust the client rect to not spill over monitor edges
                // when maximized.
                // (Thanks goes to <https://github.com/melak47/BorderlessWindow>)
                let is_maximized = unsafe {
                    let mut placement = MaybeUninit::uninit();
                    assert_win32_ok(winuser::GetWindowPlacement(hwnd, placement.as_mut_ptr()));
                    placement.assume_init().showCmd == winuser::SW_MAXIMIZE as _
                };

                if !is_maximized {
                    return 0;
                }

                let monitor =
                    unsafe { winuser::MonitorFromWindow(hwnd, winuser::MONITOR_DEFAULTTONULL) };
                if monitor.is_null() {
                    return 0;
                }

                let monitor_info = unsafe {
                    let mut monitor_info: winuser::MONITORINFO = std::mem::zeroed();
                    monitor_info.cbSize = size_of::<winuser::MONITORINFO>() as _;
                    assert_win32_ok(winuser::GetMonitorInfoW(monitor, &mut monitor_info));
                    monitor_info
                };

                ncsp.rgrc[0] = monitor_info.rcWork;
                return 0;
            }
        }

        winuser::WM_SIZE => {
            pal_hwnd.wnd.comp_wnd.handle_resize(hwnd);

            let listener = Rc::clone(&pal_hwnd.wnd.listener.borrow());
            listener.resize(wm, &pal_hwnd);
        } // WM_SIZE

        winuser::WM_MOVE => {
            pal_hwnd.wnd.text_input_wnd.on_move(wm);
        } // WM_MOVE

        _ => {}
    }

    drop(pal_hwnd);
    unsafe { winuser::DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Extract x- and y- coordinates from `LPARAM`. This is used by most types of
/// mouse input events.
///
/// If `is_screen` is `true`, the coordinates are interpreted as screen
/// coordinates, which will be converted to client coordinates by this function.
fn lparam_to_mouse_loc(hwnd: HWND, lparam: LPARAM, is_screen: bool) -> cgmath::Point2<f32> {
    let lparam = lparam as DWORD;
    let mut loc_phy = POINT {
        x: LOWORD(lparam) as i16 as LONG, // `GET_X_LPARAM(lparam) as LONG`
        y: HIWORD(lparam) as i16 as LONG, // `GET_Y_LPARAM(lparam) as LONG`
    };

    if is_screen {
        assert_win32_ok(unsafe { winuser::ScreenToClient(hwnd, &mut loc_phy) });
    }

    // Convert to logical pixels
    let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
    assert_win32_ok(dpi);

    [
        phy_to_log_f32(loc_phy.x as f32, dpi),
        phy_to_log_f32(loc_phy.y as f32, dpi),
    ]
    .into()
}

/// Calculate the physical outer size for a given logical inner size.
fn log_inner_to_phy_outer(
    hwnd: HWND,
    dpi: u32,
    size: [u32; 2],
    should_adjust_for_border: bool,
) -> [i32; 2] {
    unsafe {
        let phy_size = size.map(|i| log_to_phy(i, dpi));
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: phy_size[0] as i32,
            bottom: phy_size[1] as i32,
        };
        let style = winuser::GetWindowLongW(hwnd, winuser::GWL_STYLE) as _;
        let exstyle = winuser::GetWindowLongW(hwnd, winuser::GWL_EXSTYLE) as _;

        if should_adjust_for_border {
            assert_win32_ok(winuser::AdjustWindowRectExForDpi(
                &mut rect, style, 0, // the window doesn't have a menu
                exstyle, dpi,
            ));
        }

        [rect.right - rect.left, rect.bottom - rect.top]
    }
}

/// Convert logical client coordinates to physical screen coordinates.
fn log_client_to_phy_screen_with_dpi(
    hwnd: HWND,
    dpi: u32,
    p: cgmath::Point2<f32>,
) -> cgmath::Point2<LONG> {
    let mut loc_phy = POINT {
        x: log_to_phy_f32(p.x, dpi) as LONG,
        y: log_to_phy_f32(p.y, dpi) as LONG,
    };

    assert_win32_ok(unsafe { winuser::ClientToScreen(hwnd, &mut loc_phy) });

    [loc_phy.x, loc_phy.y].into()
}

/// Convert logical client coordinates to physical screen coordinates.
pub(super) fn log_client_box2_to_phy_screen_rect(hwnd: HWND, p: cggeom::Box2<f32>) -> RECT {
    let dpi = unsafe { winuser::GetDpiForWindow(hwnd) } as u32;
    assert_win32_ok(dpi);

    let p1 = log_client_to_phy_screen_with_dpi(hwnd, dpi, p.min);
    let p2 = log_client_to_phy_screen_with_dpi(hwnd, dpi, p.max);

    RECT {
        left: p1.x,
        top: p1.y,
        right: p2.x,
        bottom: p2.y,
    }
}

fn phy_to_log(x: u32, dpi: u32) -> u32 {
    // Must be rounded up so that the drawn region (which is sized according to
    // the logical size because the user only knows the logical size) completely
    // covers a window's client region.
    (x * 96 + dpi - 1) / dpi
}

fn log_to_phy(x: u32, dpi: u32) -> u32 {
    // Must be rounded down so that `phy_to_log . log_to_phy` is an identity
    // operation when `dpi >= 96`.
    x * dpi / 96
}

fn phy_to_log_f32(x: f32, dpi: u32) -> f32 {
    x * (96.0 / dpi as f32)
}

fn log_to_phy_f32(x: f32, dpi: u32) -> f32 {
    x * (dpi as f32 / 96.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn phy_log_roundtrip(x: u16, dpi: u8) -> bool {
        let x = x as u32;
        let dpi = dpi as u32 + 96; // assume `dpi >= 96`
        phy_to_log(log_to_phy(x, dpi), dpi) == x
    }
}
