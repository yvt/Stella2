//! The window type implementation for macOS.
//!
//! The relationship of objects are shown below:
//!
//! ```text
//!  TCWWindowController -------> NSWindow
//!    ^             | ^             |
//!    |             | '-------------' (weak)
//!    |             |
//!    |             | <--- (this cycle is severed here
//!    |             |       when the window is closed)
//!    |             v
//!  HWnd <------ WndState  (HWnd == id<TCWWindowController>)
//!    ^                |
//!    |                |
//!    '- WndListener <-'
//! ```
use cocoa::{
    base::{id, nil},
    foundation::{NSNotFound, NSPoint, NSRange, NSRect, NSSize, NSString, NSUInteger},
};
use flags_macro::flags;
use objc::{msg_send, runtime::BOOL, sel, sel_impl};
use std::{
    cell::{Cell, RefCell},
    cmp::min,
    ffi::CStr,
    fmt,
    ops::Range,
    os::raw::{c_char, c_int},
    rc::Rc,
};
use utf16count::{find_utf16_pos, utf16_len};

use super::{
    drawutils::{ns_rect_from_box2, point2_from_ns_point},
    utils::with_autorelease_pool,
    HLayer, IdRef, Wm, WndAttrs,
};
use crate::iface::{self, actions, Wm as _};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct HWnd {
    /// `TCWWindowController`
    ctrler: IdRef,
}

#[derive(Clone)]
pub struct HTextInputCtx {
    inner: Rc<TextInputCtx>,
}

struct TextInputCtx {
    wnd_state: Rc<WndState>,
    listener: Box<dyn iface::TextInputCtxListener<Wm>>,
}

struct WndState {
    listener: RefCell<Box<dyn iface::WndListener<Wm>>>,
    layer: Cell<Option<HLayer>>,
    tictx: Cell<Option<Rc<TextInputCtx>>>,
    marked_range: Cell<Option<Range<usize>>>,
    hwnd: HWnd,
    /// A non-zero value indicates `[NSWindow windowShouldClose:]` is in the
    /// call stack.
    closing: Cell<u8>,
    /// `true` means `[NSWindow windowShouldClose:]` should return `true`.
    should_close: Cell<bool>,
}

impl HWnd {
    pub(super) fn new(wm: Wm, attrs: WndAttrs<'_>) -> Self {
        with_autorelease_pool(|| {
            extern "C" {
                /// Return `[TCWWindowController class]`.
                fn tcw_wnd_ctrler_cls() -> id;
            }

            let ctrler: id = unsafe { msg_send![tcw_wnd_ctrler_cls(), alloc] };
            let ctrler = IdRef::new(unsafe { msg_send![ctrler, init] })
                .non_nil()
                .unwrap();

            // Create a handle
            let this = HWnd { ctrler };

            // Create `WndState`
            let state = Rc::new(WndState {
                listener: RefCell::new(Box::new(())),
                layer: Cell::new(None),
                tictx: Cell::new(None),
                marked_range: Cell::new(None),
                hwnd: this.clone(),
                closing: Cell::new(0),
                should_close: Cell::new(false),
            });

            // Attach `WndState`
            let () = unsafe { msg_send![*this.ctrler, setListenerUserData: Rc::into_raw(state)] };

            this.set_attrs(wm, attrs);
            let () = unsafe { msg_send![*this.ctrler, center] };

            this
        })
    }

    /// Return a smart pointer to the `WndState` associated with this `HWnd`.
    fn state(&self) -> Rc<WndState> {
        unsafe {
            let ud: *const WndState = msg_send![*self.ctrler, listenerUserData];
            let rc = Rc::from_raw(ud);
            Rc::into_raw(Rc::clone(&rc));
            rc
        }
    }

    pub(super) fn set_attrs(&self, wm: Wm, attrs: WndAttrs<'_>) {
        let state = self.state();

        // Call `setFlags` before `setContentSize` to make sure the window
        // properly sized based on the target window style masks
        if let Some(value) = attrs.flags {
            let () = unsafe { msg_send![*self.ctrler, setFlags: value.bits()] };
        }

        if let Some(value) = attrs.size {
            let size = NSSize::new(value[0] as _, value[1] as _);
            let () = unsafe { msg_send![*self.ctrler, setContentSize: size] };
        }

        if let Some(value) = attrs.min_size {
            let min_size = NSSize::new(value[0] as _, value[1] as _);
            let () = unsafe { msg_send![*self.ctrler, setContentMinSize: min_size] };
        }

        if let Some(value) = attrs.max_size {
            let max_size = NSSize::new(value[0] as _, value[1] as _);
            let () = unsafe { msg_send![*self.ctrler, setContentMaxSize: max_size] };
        }

        if let Some(value) = &attrs.caption {
            let title = IdRef::new(unsafe { NSString::alloc(nil).init_str(&**value) });
            let () = unsafe { msg_send![*self.ctrler, setTitle:*title] };
        }

        match attrs.visible {
            Some(true) => {
                let () = unsafe { msg_send![*self.ctrler, makeKeyAndOrderFront] };
            }
            Some(false) => {
                let () = unsafe { msg_send![*self.ctrler, orderOut] };
            }
            None => {}
        }

        if let Some(value) = attrs.listener {
            state.listener.replace(value);
        }

        if let Some(value) = attrs.layer {
            let layer = if let Some(hlayer) = &value {
                hlayer.ca_layer(wm)
            } else {
                nil
            };
            let () = unsafe { msg_send![*self.ctrler, setLayer: layer] };
            state.layer.set(value);
        }

        if let Some(value) = attrs.cursor_shape {
            let value = value as u32;
            let () = unsafe { msg_send![*self.ctrler, setCursorShape: value] };
        }
    }

    pub(super) fn remove(&self, _: Wm) {
        let state = self.state();
        if state.closing.get() > 0 {
            // `[NSWindow windowShouldClose:]` is currently being called. We
            // should return `true` from that method and shouldn't call `close`
            // here.
            //
            // Calling `close` here causes a random memory corruption (bug in
            // Cocoa?).
            state.should_close.set(true);
            return;
        }

        with_autorelease_pool(|| {
            let () = unsafe { msg_send![*self.ctrler, close] };
        })
    }

    pub(super) fn update(&self, _: Wm) {
        // The system automatically commits any implicit transaction
    }

    pub(super) fn request_update_ready(&self, _wm: Wm) {
        let () = unsafe { msg_send![*self.ctrler, requestUpdateReady] };
    }

    pub(super) fn get_size(&self, _: Wm) -> [u32; 2] {
        let size: NSSize = unsafe { msg_send![*self.ctrler, contentSize] };
        [size.width as u32, size.height as u32]
    }

    pub(super) fn get_dpi_scale(&self, _: Wm) -> f32 {
        unsafe { msg_send![*self.ctrler, dpiScale] }
    }

    pub(super) fn is_focused(&self, _: Wm) -> bool {
        let value: BOOL = unsafe { msg_send![*self.ctrler, isKeyWindow] };
        value != 0
    }
}

// ---------------------------------------------------------------------------
// `HTextInputCtx`

impl fmt::Debug for HTextInputCtx {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("HTextInputCtx")
            .field("inner", &(&*self.inner as *const _))
            .finish()
    }
}

impl PartialEq for HTextInputCtx {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for HTextInputCtx {}

impl std::hash::Hash for HTextInputCtx {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (&*self.inner as *const TextInputCtx).hash(state);
    }
}

impl HTextInputCtx {
    pub(super) fn new(hwnd: HWnd, listener: Box<dyn iface::TextInputCtxListener<Wm>>) -> Self {
        Self {
            inner: Rc::new(TextInputCtx {
                wnd_state: hwnd.state(),
                listener,
            }),
        }
    }

    pub(super) fn set_active(&self, active: bool) {
        // `HWnd` only can be created in a main thread and `HTextInputCtx`
        // contains a reference to one, so this is safe
        let wm = unsafe { Wm::global_unchecked() };

        let tictx_cell = &self.inner.wnd_state.tictx;
        let wnd_ctrler = *self.inner.wnd_state.hwnd.ctrler;
        let mut cur_tictx = tictx_cell.take();
        let cur_active = option_deref_to_ptr(&cur_tictx) == (&*self.inner) as *const _;

        if cur_active != active {
            // Deactivate `cur_tictx` first.
            if let Some(inner) = cur_tictx {
                // Notify the listener that we are no longer interested in
                // receiving events regarding `cur_tictx`. `set_event_mask`
                // takes `&HTextInputCtx`, so we first make one from `cur_tictx`.
                let htictx = HTextInputCtx { inner };
                htictx.inner.listener.set_event_mask(
                    wm,
                    &htictx,
                    iface::TextInputCtxEventFlags::empty(),
                );

                // After the notification is done, destruct `HTextInputCtx` and
                // take `cur_tictx` back.
                cur_tictx = Some(htictx.inner);
            }
            // Put `cur_tictx` back...
            std::mem::forget(tictx_cell.replace(cur_tictx));

            // And reset the text input. This will send clean-up events to
            // `cur_tictx`.
            let () = unsafe { msg_send![wnd_ctrler, resetTextInput] };

            // Determine the next active context
            if active {
                // Activate `self`
                cur_tictx = Some(Rc::clone(&self.inner));

                // Notify the listener that we are now interested in
                // receiving events regarding `cur_tictx`.
                self.inner.listener.set_event_mask(
                    wm,
                    self,
                    flags![iface::TextInputCtxEventFlags::{RESET | SELECTION_CHANGE | LAYOUT_CHANGE}],
                );

                let () = unsafe { msg_send![wnd_ctrler, resetTextInput] };
            } else {
                // No active context
                cur_tictx = None;
            }
        }

        // Put back `cur_tictx`
        std::mem::forget(tictx_cell.replace(cur_tictx));
    }

    fn is_active(&self) -> bool {
        cell_map(&self.inner.wnd_state.tictx, |cur_tictx| {
            option_deref_to_ptr(&cur_tictx) == (&*self.inner) as *const _
        })
    }

    pub(super) fn reset(&self) {
        if self.is_active() {
            let wnd_ctrler = *self.inner.wnd_state.hwnd.ctrler;
            let () = unsafe { msg_send![wnd_ctrler, resetTextInput] };

            self.inner.wnd_state.marked_range.set(None);
        }
    }

    pub(super) fn on_selection_change(&self) {
        self.reset();
    }

    pub(super) fn on_layout_change(&self) {
        // It's harmless to do this when we don't have to. It's up to the client
        // to track event flags and call `on_layout_change` only when required.
        let wnd_ctrler = *self.inner.wnd_state.hwnd.ctrler;
        let () = unsafe { msg_send![wnd_ctrler, repositionTextInput] };
    }
}

// ---------------------------------------------------------------------------
// Accelerator tables
//
// Most of these types are implementation details and thus hidden. They still
// need to be `pub` because they are instantiated by `accel_table!`.

#[derive(Debug)]
pub struct AccelTable {
    #[doc(hidden)]
    pub key: &'static [ActionKeyBinding],
    #[doc(hidden)]
    pub sel: &'static [ActionSelBinding],
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ActionKeyBinding {
    pub action: iface::ActionId,
    pub mod_mask: u16,
    pub mod_flags: u16,
    pub charcode: u16,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ActionSelBinding {
    pub action: iface::ActionId,
    pub sel: &'static str,
}

impl AccelTable {
    fn find_action_with_key(&self, mod_flags: u16, charcode_unmod: u16) -> Option<iface::ActionId> {
        self.key
            .iter()
            .filter(move |binding| {
                (binding.mod_mask & mod_flags) == binding.mod_flags
                    && charcode_unmod == binding.charcode
            })
            .map(|binding| binding.action)
            .nth(0)
    }

    fn find_action_with_sel(&self, sel: &[u8]) -> Option<iface::ActionId> {
        self.sel
            .iter()
            .filter(|binding| binding.sel.as_bytes() == sel)
            .map(|binding| binding.action)
            .nth(0)
    }
}

// ---------------------------------------------------------------------------
// Utility functions

fn option_deref_to_ptr<T>(x: &Option<impl std::ops::Deref<Target = T>>) -> *const T {
    if let Some(x) = x {
        &**x as *const T
    } else {
        std::ptr::null()
    }
}

fn cell_map<T: Default, R>(cell: &Cell<T>, map: impl FnOnce(&mut T) -> R) -> R {
    let mut val = cell.take();
    let ret = map(&mut val);
    cell.set(val);
    ret
}

fn cell_get_by_clone<T: Clone + Default>(cell: &Cell<T>) -> T {
    cell_map(cell, |inner| inner.clone())
}

fn sort_range(r: Range<usize>) -> Range<usize> {
    if r.end < r.start {
        r.end..r.start
    } else {
        r
    }
}

// ---------------------------------------------------------------------------

// These functions are called by `TCWWindowController`
type TCWListenerUserData = *const WndState;

unsafe fn method_impl<T>(ud: TCWListenerUserData, f: impl FnOnce(Wm, &WndState) -> T) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = Wm::global_unchecked();
    Some(f(wm, &*ud))
}

// TODO: catch panics

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_should_close(ud: TCWListenerUserData) -> BOOL {
    method_impl(ud, |wm, state| {
        state.closing.set(state.closing.get() + 1);
        state.listener.borrow().close_requested(wm, &state.hwnd);
        state.closing.set(state.closing.get() - 1);

        state.should_close.get() as BOOL
    })
    .unwrap_or(1)
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_close(ud: TCWListenerUserData) {
    method_impl(ud, |_wm, state| {
        // Detach the listener from the controller
        let () = msg_send![*state.hwnd.ctrler, setListenerUserData: nil];
    });

    if !ud.is_null() {
        Rc::from_raw(ud);
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_resize(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().resize(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_dpi_scale_changed(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().dpi_scale_changed(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_focus(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().focus(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_update_ready(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().update_ready(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_motion(ud: TCWListenerUserData, loc: NSPoint) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().mouse_motion(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_leave(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().mouse_leave(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_nc_hit_test(ud: TCWListenerUserData, loc: NSPoint) -> c_int {
    method_impl(ud, |wm, state| {
        state.listener.borrow().nc_hit_test(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
        )
    })
    .unwrap_or(iface::NcHit::Client) as c_int
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_mouse_drag(
    ud: TCWListenerUserData,
    loc: NSPoint,
    button: u8,
) -> TCWMouseDragListenerUserData {
    method_impl(ud, |wm, state| {
        let listener = state.listener.borrow().mouse_drag(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            button,
        );

        let state = DragState {
            listener,
            hwnd: state.hwnd.clone(),
        };

        Box::into_raw(Box::new(state)) as *const _
    })
    .unwrap_or(std::ptr::null())
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_scroll_motion(
    ud: TCWListenerUserData,
    loc: NSPoint,
    precise: u8,
    delta_x: f64,
    delta_y: f64,
) {
    method_impl(ud, |wm, state| {
        state.listener.borrow().scroll_motion(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            &iface::ScrollDelta {
                precise: precise != 0,
                delta: [delta_x as f32, delta_y as f32].into(),
            },
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_scroll_gesture(
    ud: TCWListenerUserData,
    loc: NSPoint,
) -> TCWScrollListenerUserData {
    method_impl(ud, |wm, state| {
        let listener = state.listener.borrow().scroll_gesture(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
        );

        let state = ScrollState {
            listener,
            hwnd: state.hwnd.clone(),
        };

        Box::into_raw(Box::new(state)) as *const _
    })
    .unwrap_or(std::ptr::null())
}

// ---------------------------------------------------------------------------

struct EnumAccel<F: FnMut(&AccelTable)>(F);

impl<F: FnMut(&AccelTable)> iface::InterpretEventCtx<AccelTable> for EnumAccel<F> {
    fn use_accel(&mut self, accel: &AccelTable) {
        (self.0)(accel);
    }
}

struct KeyEvent {
    mod_flags: u16,
    charcode_unmod: u16,
}

impl iface::KeyEvent<AccelTable> for KeyEvent {
    fn translate_accel(&self, accel_table: &AccelTable) -> Option<iface::ActionId> {
        accel_table.find_action_with_key(self.mod_flags, self.charcode_unmod)
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_key_down(
    ud: TCWListenerUserData,
    mod_flags: u16,
    charcode_unmod: u16,
) -> c_int {
    method_impl(ud, |wm, state| {
        log::trace!("tcw_wndlistener_key_down{:?}", (mod_flags, charcode_unmod));

        let listener = state.listener.borrow();

        let mut action = None;
        let action_ref = &mut action;
        listener.interpret_event(
            wm,
            &state.hwnd,
            &mut EnumAccel(move |accel_table| {
                if action_ref.is_none() {
                    *action_ref = accel_table.find_action_with_key(mod_flags, charcode_unmod);
                }
            }),
        );

        log::trace!("... action = {:?}", action);

        if let Some(action) = action {
            // The action was found. Can the window handle it?
            let status = listener.validate_action(wm, &state.hwnd, action);
            if !status.contains(flags![iface::ActionStatus::{VALID | ENABLED}]) {
                return 0;
            }

            listener.perform_action(wm, &state.hwnd, action);

            return 1; // Handled
        }

        let handled = listener.key_down(
            wm,
            &state.hwnd,
            &KeyEvent {
                mod_flags,
                charcode_unmod,
            },
        );
        log::trace!("... key_down(...) = {:?}", handled);

        handled as _
    })
    .unwrap_or(0)
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_key_up(
    ud: TCWListenerUserData,
    mod_flags: u16,
    charcode_unmod: u16,
) -> c_int {
    method_impl(ud, |wm, state| {
        log::trace!("tcw_wndlistener_key_up{:?}", (mod_flags, charcode_unmod));

        let listener = state.listener.borrow();

        let handled = listener.key_up(
            wm,
            &state.hwnd,
            &KeyEvent {
                mod_flags,
                charcode_unmod,
            },
        );
        log::trace!("... key_up(...) = {:?}", handled);

        handled as _
    })
    .unwrap_or(0)
}

fn listener_map_sel(
    wm: Wm,
    hwnd: &HWnd,
    listener: &dyn iface::WndListener<Wm>,
    sel: &[u8],
) -> Option<iface::ActionId> {
    let mut action = None;
    let action_ref = &mut action;
    listener.interpret_event(
        wm,
        hwnd,
        &mut EnumAccel(move |accel_table| {
            if action_ref.is_none() {
                *action_ref = accel_table.find_action_with_sel(sel);
            }
        }),
    );
    action
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_validate_selector(
    ud: TCWListenerUserData,
    selector: *const c_char,
    selector_len: usize,
) -> u8 {
    method_impl(ud, |wm, state| {
        let sel = std::slice::from_raw_parts(selector as *const u8, selector_len);
        log::trace!(
            "tcw_wndlistener_validate_selector({:?})",
            std::str::from_utf8(sel)
        );

        let listener = state.listener.borrow();

        let action = listener_map_sel(wm, &state.hwnd, &**listener, sel);
        log::trace!("... action = {:?}", action);

        if let Some(action) = action {
            // The action was found. Can the window handle it?
            listener.validate_action(wm, &state.hwnd, action).bits()
        } else {
            0 // Invalid
        }
    })
    .unwrap_or(0)
}

#[no_mangle]
unsafe extern "C" fn tcw_wndlistener_perform_selector(
    ud: TCWListenerUserData,
    selector: *const c_char,
    selector_len: usize,
) {
    method_impl(ud, |wm, state| {
        let sel = std::slice::from_raw_parts(selector as *const u8, selector_len);
        log::trace!(
            "tcw_wndlistener_perform_selector({:?})",
            std::str::from_utf8(sel)
        );

        let listener = state.listener.borrow();

        let action = listener_map_sel(wm, &state.hwnd, &**listener, sel);
        log::trace!("... action = {:?}", action);

        if let Some(action) = action {
            listener.perform_action(wm, &state.hwnd, action);
        }
    });
}

static TEXT_INPUT_ACCEL: AccelTable = accel_table_inner!(
    crate,
    "macos",
    [
        (actions::SELECT_ALL, macos_sel("selectAll:")),
        (actions::SELECT_LINE, macos_sel("selectLine:")),
        (actions::SELECT_PARAGRAPH, macos_sel("selectParagraph:")),
        (actions::SELECT_WORD, macos_sel("selectWord:")),
        (actions::UPPERCASE_WORD, macos_sel("uppercaseWord:")),
        (actions::LOWERCASE_WORD, macos_sel("lowercaseWord:")),
        (actions::CAPITALIZE_WORD, macos_sel("capitalizeWord:")),
        (actions::DELETE_BACKWARD, macos_sel("deleteBackward:")),
        (
            actions::DELETE_BACKWARD_DECOMPOSING,
            macos_sel("deleteBackwardByDecomposingPreviousCharacter:")
        ),
        (
            actions::DELETE_BACKWARD_WORD,
            macos_sel("deleteWordBackward:")
        ),
        (actions::DELETE_FORWARD, macos_sel("deleteForward:")),
        (
            actions::DELETE_FORWARD_WORD,
            macos_sel("deleteWordForward:")
        ),
        (actions::INSERT_LINE_BREAK, macos_sel("insertLineBreak:")),
        (actions::INSERT_PARAGRAPH_BREAK, macos_sel("insertNewline:")),
        (
            actions::INSERT_PARAGRAPH_BREAK,
            macos_sel("insertParagraphSeparator:")
        ),
        (actions::INSERT_TAB, macos_sel("insertTab:")),
        (actions::INSERT_BACKTAB, macos_sel("insertBacktab:")),
        (actions::MOVE_BACKWARD, macos_sel("moveBackward:")),
        (actions::MOVE_FORWARD, macos_sel("moveForward:")),
        (actions::MOVE_LEFT, macos_sel("moveLeft:")),
        (actions::MOVE_RIGHT, macos_sel("moveRight:")),
        (actions::MOVE_BACKWARD_WORD, macos_sel("moveWordBackward:")),
        (actions::MOVE_FORWARD_WORD, macos_sel("moveWordForward:")),
        (actions::MOVE_LEFT_WORD, macos_sel("moveWordLeft:")),
        (actions::MOVE_RIGHT_WORD, macos_sel("moveWordRight:")),
        (
            actions::MOVE_START_OF_LINE,
            macos_sel("moveToBeginningOfLine:")
        ),
        (actions::MOVE_END_OF_LINE, macos_sel("moveToEndOfLine:")),
        (
            actions::MOVE_LEFT_END_OF_LINE,
            macos_sel("moveToLeftEndOfLine:")
        ),
        (
            actions::MOVE_RIGHT_END_OF_LINE,
            macos_sel("moveToRightEndOfLine:")
        ),
        (
            actions::MOVE_START_OF_PARAGRAPH,
            macos_sel("moveToBeginningOfParagraph:")
        ),
        (
            actions::MOVE_END_OF_PARAGRAPH,
            macos_sel("moveToEndOfParagraph:")
        ),
        (
            actions::MOVE_START_OF_DOCUMENT,
            macos_sel("moveToBeginningOfDocument:")
        ),
        (
            actions::MOVE_END_OF_DOCUMENT,
            macos_sel("moveToEndOfDocument:")
        ),
        (actions::MOVE_UP, macos_sel("moveUp:")),
        (actions::MOVE_DOWN, macos_sel("moveDown:")),
        (actions::MOVE_UP_PAGE, macos_sel("pageUp:")),
        (actions::MOVE_DOWN_PAGE, macos_sel("pageDown:")),
        (
            actions::MOVE_BACKWARD_SELECTING,
            macos_sel("moveBackwardAndModifySelection:")
        ),
        (
            actions::MOVE_FORWARD_SELECTING,
            macos_sel("moveForwardAndModifySelection:")
        ),
        (
            actions::MOVE_LEFT_SELECTING,
            macos_sel("moveLeftAndModifySelection:")
        ),
        (
            actions::MOVE_RIGHT_SELECTING,
            macos_sel("moveRightAndModifySelection:")
        ),
        (
            actions::MOVE_BACKWARD_WORD_SELECTING,
            macos_sel("moveBackwardAndModifySelection:")
        ),
        (
            actions::MOVE_FORWARD_WORD_SELECTING,
            macos_sel("moveForwardAndModifySelection:")
        ),
        (
            actions::MOVE_LEFT_WORD_SELECTING,
            macos_sel("moveWordLeftAndModifySelection:")
        ),
        (
            actions::MOVE_RIGHT_WORD_SELECTING,
            macos_sel("moveWordRightAndModifySelection:")
        ),
        (
            actions::MOVE_START_OF_LINE_SELECTING,
            macos_sel("moveToBeginningOfLineAndModifySelection:")
        ),
        (
            actions::MOVE_END_OF_LINE_SELECTING,
            macos_sel("moveToEndOfLineAndModifySelection:")
        ),
        (
            actions::MOVE_LEFT_END_OF_LINE_SELECTING,
            macos_sel("moveToLeftEndOfLineAndModifySelection:")
        ),
        (
            actions::MOVE_RIGHT_END_OF_LINE_SELECTING,
            macos_sel("moveToRightEndOfLineAndModifySelection:")
        ),
        (
            actions::MOVE_START_OF_PARAGRAPH_SELECTING,
            macos_sel("moveToBeginningOfParagraphAndModifySelection:")
        ),
        (
            actions::MOVE_END_OF_PARAGRAPH_SELECTING,
            macos_sel("moveToEndOfParagraphAndModifySelection:")
        ),
        (
            actions::MOVE_START_OF_DOCUMENT_SELECTING,
            macos_sel("moveToBeginningOfDocumentAndModifySelection:")
        ),
        (
            actions::MOVE_END_OF_DOCUMENT_SELECTING,
            macos_sel("moveToEndOfDocumentAndModifySelection:")
        ),
        (
            actions::MOVE_UP_SELECTING,
            macos_sel("moveUpAndModifySelection:")
        ),
        (
            actions::MOVE_DOWN_SELECTING,
            macos_sel("moveDownAndModifySelection:")
        ),
        (
            actions::MOVE_UP_PAGE_SELECTING,
            macos_sel("pageUpAndModifySelection:")
        ),
        (
            actions::MOVE_DOWN_PAGE_SELECTING,
            macos_sel("pageDownAndModifySelection:")
        ),
        (actions::SCROLL_UP, macos_sel("scrollLineUp:")),
        (actions::SCROLL_DOWN, macos_sel("scrollLineDown:")),
        (actions::SCROLL_UP_PAGE, macos_sel("scrollPageUp:")),
        (actions::SCROLL_DOWN_PAGE, macos_sel("scrollPageDown:")),
        (
            actions::SCROLL_START_OF_DOCUMENT,
            macos_sel("scrollToBeginningOfDocument:")
        ),
        (
            actions::SCROLL_END_OF_DOCUMENT,
            macos_sel("scrollToEndOfDocument:")
        ),
    ]
);

#[no_mangle]
unsafe extern "C" fn tcw_wnd_perform_text_input_selector(
    ud: TCWListenerUserData,
    selector: *const c_char,
    selector_len: usize,
) {
    method_impl(ud, |wm, state| {
        let sel = std::slice::from_raw_parts(selector as *const u8, selector_len);
        log::trace!(
            "tcw_wnd_perform_text_input_selector({:?})",
            std::str::from_utf8(sel)
        );

        let listener = state.listener.borrow();

        let action = TEXT_INPUT_ACCEL.find_action_with_sel(sel);
        log::trace!("... action = {:?}", action);

        if let Some(action) = action {
            listener.perform_action(wm, &state.hwnd, action);
        }
    });
}

// ---------------------------------------------------------------------------

/// Convert `range` to UTF-8. The converted back UTF-16 range will be returned as
/// the second value. A prefix of the document containing the range will be
/// returned as the third value.
fn edit_convert_range_to_utf8_with_text(
    edit: &mut dyn iface::TextInputCtxEdit<Wm>,
    range: Range<usize>,
) -> (Range<usize>, Range<usize>, String) {
    let (start, end) = (range.start, range.end);

    // Each UTF-16 unit maps to 1–3 three UTF-8-encoded bytes. Based on
    // this fact, we can find the upper bound.
    let aperture = min(end.saturating_mul(3), edit.len());
    let aperture = edit.floor_index(aperture);
    let text = edit.slice(0..aperture);

    let result = find_utf16_pos(start, &text);
    let start_u8 = result.utf8_cursor;
    let start_actual = start - result.utf16_extra;

    let result = find_utf16_pos(end - start_actual, &text[start_u8..]);
    let end_u8 = start_u8 + result.utf8_cursor;
    let end_actual = end - result.utf16_extra;

    (start_u8..end_u8, start_actual..end_actual, text)
}

/// `edit_convert_range_to_utf8_with_text` without the third value.
fn edit_convert_range_to_utf8(
    edit: &mut dyn iface::TextInputCtxEdit<Wm>,
    range: Range<usize>,
) -> (Range<usize>, Range<usize>) {
    let (range_u8, range_u16, _) = edit_convert_range_to_utf8_with_text(edit, range);
    (range_u8, range_u16)
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_has_text_input_ctx(ud: TCWListenerUserData) -> c_int {
    method_impl(ud, |_, state| {
        // Return `1` iff `state.tictx` contains `Some(_)`
        cell_map(&state.tictx, |cur_tictx| cur_tictx.is_some() as c_int)
    })
    .unwrap_or(0)
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_insert_text(
    ud: TCWListenerUserData,
    st: *const c_char,
    replace_start: usize,
    replace_len: usize,
) {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `insertText` without an active TI context");
            return;
        };

        let st = if let Ok(st) = CStr::from_ptr(st).to_str() {
            st
        } else {
            log::warn!("Rejecting `insertText` because of a malformed UTF-8 string");
            return;
        };

        log::trace!(
            "tcw_wnd_insert_text: replace={}..+{}, st={:?}",
            replace_start,
            replace_len,
            st
        );

        let has_control_char = st.bytes().any(|b| b.is_ascii_control() && b != b'\r');

        if has_control_char {
            log::warn!(
                "Rejecting `insertText` because of the inclusion of a disallowed control character"
            );
            return;
        }

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            true,
        );

        let replace_range = if replace_start >= i32::max_value() as usize {
            // `NSNotFound`
            // This case is undocumented, but it seems that it means
            // the whole marked text or selected text should be finalized
            // and replaced with the given string.
            if let Some(range) = state.marked_range.take() {
                range
            } else {
                sort_range(edit.selected_range())
            }
        } else {
            if let Some(range) = state.marked_range.take() {
                log::warn!(
                    "Don't know how to handle `insertText` with a non-`NSNotFound` \
                    range. Clearing the composition range ({:?}) first.",
                    range
                );
            }

            // Convert `replace_start..replace_start + replace_len` to UTF-8.
            edit_convert_range_to_utf8(
                &mut *edit,
                replace_start..replace_start.saturating_add(replace_len),
            )
            .0
        };

        // Insert the text
        edit.replace(replace_range.clone(), st);

        // clear the composition range
        edit.set_composition_range(None);

        // Move the caret next to the inserted text
        let new_sel_i = replace_range.start + st.len();
        edit.set_selected_range(new_sel_i..new_sel_i);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_set_marked_text(
    ud: TCWListenerUserData,
    st: *const c_char,
    sel_start: usize,
    sel_len: usize,
    replace_start: usize,
    replace_len: usize,
) {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `setMarkedText` without an active TI context");
            return;
        };

        let st = if let Ok(st) = CStr::from_ptr(st).to_str() {
            st
        } else {
            log::warn!("Rejecting `setMarkedText` because of a malformed UTF-8 string");
            return;
        };

        log::trace!(
            "tcw_wnd_set_marked_text: sel={}..+{}, replace={}..+{}, st={:?}",
            sel_start,
            sel_len,
            replace_start,
            replace_len,
            st
        );

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            true,
        );

        let (old_marked_range, replace_range) =
            if let Some(old_marked_range) = cell_get_by_clone(&state.marked_range) {
                // Determine which part of `old_marked_range` is to be replaced
                let replace_range = if replace_start >= i32::max_value() as usize {
                    // `NSNotFound`
                    // This case is undocumented, but it seems that it means
                    // the whole marked text should be replaced with the given string.
                    old_marked_range.clone()
                } else {
                    // Convert `replace_start..replace_start + replace_len` to UTF-8
                    let marked_str = edit.slice(old_marked_range.clone());
                    let start = find_utf16_pos(replace_start, &marked_str).utf8_cursor;
                    let end = start + find_utf16_pos(replace_len, &marked_str[start..]).utf8_cursor;
                    (start + old_marked_range.start)..(end + old_marked_range.start)
                };

                (old_marked_range, replace_range)
            } else {
                // If there's no marked text, clear the selected text first.
                if replace_start >= i32::max_value() as usize {
                    // `NSNotFound`: replace the selection
                    let sel_range = sort_range(edit.selected_range());

                    (sel_range.clone(), sel_range)
                } else {
                    // This is undocumented, but it seems that `replace_(start|len)`
                    // represents the replacement range in the text document in this
                    // case.

                    // Convert `replace_start..replace_start + replace_len` to UTF-8.
                    let replace_range = edit_convert_range_to_utf8(
                        &mut *edit,
                        replace_start..replace_start.saturating_add(replace_len),
                    )
                    .0;

                    (replace_range.clone(), replace_range)
                }
            };

        // Replace that part
        edit.replace(replace_range.clone(), st);

        // Update the composition range
        let new_marked_range =
            old_marked_range.start..old_marked_range.end - replace_range.len() + st.len();
        let new_marked_range = if new_marked_range.len() == 0 {
            None
        } else {
            Some(new_marked_range)
        };
        edit.set_composition_range(new_marked_range.clone());
        state.marked_range.set(new_marked_range);

        // Update the selection
        let sel_range = {
            // Convert `sel_start..sel_start + sel_len` to UTF-8
            let start = find_utf16_pos(sel_start, st).utf8_cursor;
            let end = start + find_utf16_pos(sel_len, &st[start..]).utf8_cursor;
            (start + replace_range.start)..(end + replace_range.start)
        };
        edit.set_selected_range(sel_range);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_unmark_text(ud: TCWListenerUserData) {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `setMarkedText` without an active TI context");
            return;
        };

        log::trace!(
            "tcw_wnd_unmark_text: marked_range was {:?}",
            cell_get_by_clone(&state.marked_range)
        );

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            true,
        );

        edit.set_composition_range(None);
        state.marked_range.set(None);
    });
}

fn nsrange_not_found() -> NSRange {
    NSRange::new(NSNotFound as NSUInteger, 0)
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_get_selected_range(ud: TCWListenerUserData) -> NSRange {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `selectedRange` without an active TI context");
            return nsrange_not_found();
        };

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            false,
        );

        let range = sort_range(edit.selected_range());

        // Convert `range` to UTF-16
        let prefix = edit.slice(0..range.end);

        debug_assert_eq!(prefix.len(), range.end);

        let start = utf16_len(&prefix[0..range.start]);
        let len = utf16_len(&prefix[range.start..]);

        log::trace!("tcw_wnd_get_selected_range → {}..+{}", start, len);
        NSRange::new(start as _, len as _)
    })
    .unwrap_or(nsrange_not_found())
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_get_marked_range(ud: TCWListenerUserData) -> NSRange {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `markedRange` without an active TI context");
            return nsrange_not_found();
        };

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            false,
        );

        let range = if let Some(range) = cell_get_by_clone(&state.marked_range) {
            range
        } else {
            return nsrange_not_found();
        };

        // Convert `range` to UTF-16
        let prefix = edit.slice(0..range.end);

        let start = utf16_len(&prefix[0..range.start]);
        let len = utf16_len(&prefix[range.start..]);

        log::trace!("tcw_wnd_get_marked_range → {}..+{}", start, len);
        NSRange::new(start as _, len as _)
    })
    .unwrap_or(nsrange_not_found())
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_get_text(
    ud: TCWListenerUserData,
    start: usize,
    len: usize,
    actual_range: Option<&mut NSRange>,
) -> id {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!(
                "Received `attributedSubstringForProposedRange` without an active TI context"
            );
            return std::ptr::null_mut();
        };

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            false,
        );

        log::trace!("tcw_wnd_get_text({}..+{})", start, len);

        // Convert `start..start + len` to UTF-8.
        let (range_u8, range_actual_u16, text) =
            edit_convert_range_to_utf8_with_text(&mut *edit, start..start.saturating_add(len));

        log::trace!("... actual range (UTF-8) = {:?}", range_u8.clone());
        log::trace!("... actual range (UTF-16) = {:?}", range_actual_u16.clone());

        if let Some(actual_range_cell) = actual_range {
            *actual_range_cell = NSRange::new(
                range_actual_u16.start as _,
                (range_actual_u16.end - range_actual_u16.start) as _,
            );
        }

        // Slice the text
        let slice = &text[range_u8];
        log::trace!("... text = {:?}", slice);

        NSString::alloc(nil).init_str(slice)
    })
    .unwrap_or_else(|| std::ptr::null_mut())
}

fn empty_nsrect() -> NSRect {
    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0))
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_get_text_rect(
    ud: TCWListenerUserData,
    start: usize,
    len: usize,
    actual_range: Option<&mut NSRange>,
) -> NSRect {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `firstRectForCharacterRange` without an active TI context");
            return empty_nsrect();
        };

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            false,
        );

        log::trace!("tcw_wnd_get_text_rect({}..+{})", start, len);

        // Convert `start..start + len` to UTF-8.
        let (range_u8, range_actual_u16, text) =
            edit_convert_range_to_utf8_with_text(&mut *edit, start..start.saturating_add(len));

        log::trace!("... actual range (UTF-8) = {:?}", range_u8);

        // Query the bounding rectangle. This may return a shorter range
        // `range_u8.start .. head_end_u8`
        let (cg_bounds, head_end_u8) = edit.slice_bounds(range_u8.clone());
        log::trace!(
            "... new actual range (UTF-8) = {:?}",
            range_u8.start..head_end_u8
        );
        log::trace!("... bounds = {}", cg_bounds.display_im());
        if range_u8.start == range_u8.end {
            debug_assert!(head_end_u8 == range_u8.start);
        } else {
            debug_assert!(head_end_u8 > range_u8.start);
            debug_assert!(head_end_u8 <= range_u8.end);
        }

        log::trace!("... (text) = {:?}", &text[range_u8.start..head_end_u8]);

        if let Some(actual_range_cell) = actual_range {
            let start_actual_u8 = range_actual_u16.start;
            let end2_actual_u8 = start_actual_u8 + utf16_len(&text[range_u8.start..head_end_u8]);
            log::trace!(
                "... new actual range (UTF-16) = {:?}",
                start_actual_u8..end2_actual_u8
            );

            *actual_range_cell = NSRange::new(
                start_actual_u8 as _,
                (end2_actual_u8 - start_actual_u8) as _,
            );
        }

        // Convert `Box2<f32>` to `NSRect`. Our Objective-C handler method
        // handles the conversion to screen coordinates.
        let bounds = ns_rect_from_box2(cg_bounds.cast::<f64>().unwrap());

        bounds
    })
    .unwrap_or(empty_nsrect())
}

#[no_mangle]
unsafe extern "C" fn tcw_wnd_get_char_index_from_point(
    ud: TCWListenerUserData,
    loc: NSPoint,
) -> NSUInteger {
    method_impl(ud, |wm, state| {
        let tictx = if let Some(tictx) = cell_map(&state.tictx, |cur_tictx| cur_tictx.clone()) {
            tictx
        } else {
            log::warn!("Received `firstRectForCharacterRange` without an active TI context");
            return NSNotFound as NSUInteger;
        };

        log::trace!("tcw_wnd_get_char_index_from_point({:?})", (loc.x, loc.y));

        let mut edit = tictx.listener.edit(
            wm,
            &HTextInputCtx {
                inner: tictx.clone(),
            },
            false,
        );

        let i = if let Some(i) = edit.index_from_point(
            point2_from_ns_point(loc).cast::<f32>().unwrap(),
            iface::IndexFromPointFlags::empty(),
        ) {
            log::trace!("... → {} (UTF-8)", i);
            i
        } else {
            log::trace!("... → (not found)");
            return NSNotFound as NSUInteger;
        };

        // Convert `i` to UTF-16
        let prefix = edit.slice(0..i);
        let i_u16 = utf16_len(&prefix);

        log::trace!("... → {} (UTF-16)", i_u16);

        i_u16 as NSUInteger
    })
    .unwrap_or(NSNotFound as NSUInteger)
}

// ---------------------------------------------------------------------------

type TCWMouseDragListenerUserData = *const DragState;

struct DragState {
    listener: Box<dyn iface::MouseDragListener<Wm>>,
    hwnd: HWnd,
}

unsafe fn drag_method_impl<T>(
    ud: TCWMouseDragListenerUserData,
    f: impl FnOnce(Wm, &DragState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = Wm::global_unchecked();
    Some(f(wm, &*ud))
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_release(ud: TCWMouseDragListenerUserData) {
    if !ud.is_null() {
        Box::from_raw(ud as *mut DragState);
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_cancel(ud: TCWMouseDragListenerUserData) {
    drag_method_impl(ud, |wm, state| {
        state.listener.cancel(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_motion(
    ud: TCWMouseDragListenerUserData,
    loc: NSPoint,
) {
    drag_method_impl(ud, |wm, state| {
        state
            .listener
            .mouse_motion(wm, &state.hwnd, point2_from_ns_point(loc).cast().unwrap());
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_down(
    ud: TCWMouseDragListenerUserData,
    loc: NSPoint,
    button: u8,
) {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_down(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            button,
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_mousedraglistener_mouse_up(
    ud: TCWMouseDragListenerUserData,
    loc: NSPoint,
    button: u8,
) {
    drag_method_impl(ud, |wm, state| {
        state.listener.mouse_up(
            wm,
            &state.hwnd,
            point2_from_ns_point(loc).cast().unwrap(),
            button,
        );
    });
}

// ---------------------------------------------------------------------------

type TCWScrollListenerUserData = *const ScrollState;

struct ScrollState {
    listener: Box<dyn iface::ScrollListener<Wm>>,
    hwnd: HWnd,
}

unsafe fn scroll_method_impl<T>(
    ud: TCWScrollListenerUserData,
    f: impl FnOnce(Wm, &ScrollState) -> T,
) -> Option<T> {
    if ud.is_null() {
        return None;
    }
    let wm = Wm::global_unchecked();
    Some(f(wm, &*ud))
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_release(ud: TCWScrollListenerUserData) {
    if !ud.is_null() {
        Box::from_raw(ud as *mut ScrollState);
    }
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_cancel(ud: TCWScrollListenerUserData) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.cancel(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_end(ud: TCWScrollListenerUserData) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.end(wm, &state.hwnd);
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_motion(
    ud: TCWScrollListenerUserData,
    precise: u8,
    delta_x: f64,
    delta_y: f64,
    vel_x: f64,
    vel_y: f64,
) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.motion(
            wm,
            &state.hwnd,
            &iface::ScrollDelta {
                precise: precise != 0,
                delta: [delta_x as f32, delta_y as f32].into(),
            },
            [vel_x as f32, vel_y as f32].into(),
        );
    });
}

#[no_mangle]
unsafe extern "C" fn tcw_scrolllistener_start_momentum_phase(ud: TCWScrollListenerUserData) {
    scroll_method_impl(ud, |wm, state| {
        state.listener.start_momentum_phase(wm, &state.hwnd);
    });
}
