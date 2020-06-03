use cgmath::{Point2, Vector2};
use gdk::prelude::*;
use glib::{
    glib_object_wrapper, glib_wrapper,
    translate::{FromGlibPtrBorrow, FromGlibPtrFull, FromGlibPtrNone, ToGlibPtr},
};
use gtk::prelude::*;
use leakypool::{LazyToken, LeakyPool, PoolPtr, SingletonToken, SingletonTokenId};
use std::{
    cell::{Cell, RefCell, RefMut},
    num::Wrapping,
    os::raw::{c_int, c_uint},
    ptr::{null_mut, NonNull},
    rc::Rc,
};

use super::{comp, Wm, WndAttrs};
use crate::{actions, iface, prelude::*, MtSticky};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd {
    ptr: WndPoolPtr,
}

leakypool::singleton_tag!(struct Tag);
type WndPool = LeakyPool<Wnd, LazyToken<SingletonToken<Tag>>>;
type WndPoolPtr = PoolPtr<Wnd, SingletonTokenId<Tag>>;

static WNDS: MtSticky<RefCell<WndPool>, Wm> = Init::INIT;

pub(super) static COMPOSITOR: MtSticky<RefCell<comp::Compositor>, Wm> =
    MtSticky::new(RefCell::new(comp::Compositor::new()));

static DRAWING_WND: MtSticky<Cell<Option<WndPoolPtr>>, Wm> = MtSticky::new(Cell::new(None));

struct Wnd {
    gtk_wnd: gtk::Window,
    gtk_widget: WndWidget,
    comp_wnd: comp::Wnd,
    listener: Rc<dyn iface::WndListener<Wm>>,
    flags: iface::WndFlags,

    /// The last known size of the window.
    size: [i32; 2],

    tick_callback_active: bool,
    tick_callback_continue: bool,

    drag_state: Option<MouseDragState>,
    scroll_state: Option<ScrollState>,
}

struct MouseDragState {
    listener: Rc<dyn iface::MouseDragListener<Wm>>,
    pressed_buttons: u32,
}

struct ScrollState {
    listener: Rc<dyn iface::ScrollListener<Wm>>,
    history: [ScrollEvent; SCROLL_HISTORY_LEN],
    history_index: Wrapping<u8>,
    momentum: Option<MomentumScrollState>,
}

struct MomentumScrollState {
    tick_callback_id: c_uint,
    velocity: Vector2<f32>,
    last_frame_time: i64,
    elapsed_time: u32,
}

const SCROLL_HISTORY_LEN: usize = 4;
const MOMENTUM_DURATION: u32 = 600; // 600 << 10 microseconds

#[derive(Clone, Copy)]
#[repr(align(16))]
struct ScrollEvent {
    time: Wrapping<u32>,
    delta: Vector2<f32>,
}

impl HWnd {
    /// Implements `Wm::new_wnd`.
    pub(super) fn new_wnd(wm: Wm, mut attrs: WndAttrs<'_>) -> Self {
        let gtk_wnd = gtk::Window::new(gtk::WindowType::Toplevel);

        let gtk_widget = WndWidget::new(wm);

        gtk_wnd.add(&gtk_widget);
        gtk_widget.set_hexpand(true);
        gtk_widget.set_vexpand(true);

        // Do not automatically fill the background
        // TODO: Use `gdk_window_set_opaque_region` to optimize
        //       system-level compositing
        gtk_wnd.set_app_paintable(true);

        // On X11, we also have to request an RGBA visual
        if let Some(vis) = gtk_wnd.get_screen().unwrap().get_rgba_visual() {
            gtk_wnd.set_visual(Some(&vis));
        }

        let comp_wnd = COMPOSITOR
            .get_with_wm(wm)
            .borrow_mut()
            .new_wnd(attrs.layer.take().unwrap_or(None));

        let wnd = Wnd {
            gtk_wnd,
            gtk_widget,
            comp_wnd,
            flags: iface::WndFlags::default(),
            listener: Rc::new(()),
            size: [0, 0],
            tick_callback_active: false,
            tick_callback_continue: false,
            drag_state: None,
            scroll_state: None,
        };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let ptr = wnds.allocate(wnd);

        // Connect window events
        let wnd = &wnds[ptr];
        wnd.gtk_widget.wnd_ptr().set(Some(ptr));

        wnd.gtk_wnd.connect_delete_event(move |_, _| {
            let listener = {
                let wnds = WNDS.get_with_wm(wm).borrow();
                Rc::clone(&wnds[ptr].listener)
            };

            listener.close_requested(wm, &Self { ptr });

            Inhibit(true)
        });

        wnd.gtk_wnd.connect_state_flags_changed(move |_, _| {
            let listener = {
                let wnds = WNDS.get_with_wm(wm).borrow();
                Rc::clone(&wnds[ptr].listener)
            };

            listener.focus(wm, &Self { ptr });
        });

        // `set_wnd_attr` borrows `WNDS`, so unborrow it before calling that
        drop(wnds);

        let this = Self { ptr };
        this.set_wnd_attr(wm, attrs);
        this
    }

    /// Implements `Wm::set_wnd_attr`.
    pub(super) fn set_wnd_attr(&self, wm: Wm, attrs: WndAttrs<'_>) {
        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let wnd = &mut wnds[self.ptr];

        if let Some(size) = attrs.size {
            wnd.gtk_wnd.resize(size[0] as i32, size[1] as i32);
            wnd.gtk_wnd.set_default_size(size[0] as i32, size[1] as i32);
        }

        if let Some(size) = attrs.min_size {
            wnd.gtk_widget
                .set_size_request(size[0] as i32, size[1] as i32);
        }

        // TODO: `max_size`. Dimensions passed to `set_geometry_hints` must
        //       include the window title bar and the border

        if let Some(flags) = attrs.flags {
            // TODO: BORDERLESS
            wnd.gtk_wnd
                .set_resizable(flags.contains(iface::WndFlags::RESIZABLE));

            if (wnd.flags ^ flags).contains(iface::WndFlags::FULL_SIZE_CONTENT) {
                let titlebar_widget;
                wnd.gtk_wnd
                    .set_titlebar(if flags.contains(iface::WndFlags::FULL_SIZE_CONTENT) {
                        titlebar_widget = gtk::Fixed::new();
                        Some(&titlebar_widget)
                    } else {
                        None
                    });
            }

            wnd.flags = flags;
        }

        if let Some(layer) = attrs.layer {
            COMPOSITOR
                .get_with_wm(wm)
                .borrow_mut()
                .set_wnd_layer(&wnd.comp_wnd, layer);
        }

        let _old_listener;
        if let Some(listener) = attrs.listener {
            _old_listener = std::mem::replace(&mut wnd.listener, Rc::from(listener));
        }

        if let Some(shape) = attrs.cursor_shape {
            use self::iface::CursorShape;
            let name = match shape {
                CursorShape::Default => "default",
                CursorShape::Crosshair => "crosshair",
                CursorShape::Hand => "pointer",
                CursorShape::Arrow => "default",
                CursorShape::Move => "move",
                CursorShape::Text => "text",
                CursorShape::Wait => "wait",
                CursorShape::Help => "help",
                CursorShape::Progress => "progress",
                CursorShape::NotAllowed => "not-allowed",
                CursorShape::ContextMenu => "context-menu",
                CursorShape::Cell => "cell",
                CursorShape::VerticalText => "vertical-text",
                CursorShape::Alias => "alias",
                CursorShape::Copy => "copy",
                CursorShape::NoDrop => "no-drop",
                CursorShape::Grab => "grab",
                CursorShape::Grabbing => "grabbing",
                CursorShape::AllScroll => "all-scroll",
                CursorShape::ZoomIn => "zoom-in",
                CursorShape::ZoomOut => "zoom-out",
                CursorShape::EResize => "e-resize",
                CursorShape::NResize => "n-resize",
                CursorShape::NeResize => "ne-resize",
                CursorShape::NwResize => "ne-resize",
                CursorShape::SResize => "s-resize",
                CursorShape::SeResize => "se-resize",
                CursorShape::SwResize => "sw-resize",
                CursorShape::WResize => "w-resize",
                CursorShape::EwResize => "ew-resize",
                CursorShape::NsResize => "ns-resize",
                CursorShape::NeswResize => "nesw-resize",
                CursorShape::NwseResize => "nwse-resize",
                CursorShape::ColResize => "col-resize",
                CursorShape::RowResize => "row-resize",
            };

            let cursor = gdk::Cursor::new_from_name(&wnd.gtk_widget.get_display().unwrap(), name);

            wnd.gtk_wnd
                .get_window()
                .unwrap()
                .set_cursor(cursor.as_ref());
        }

        if let Some(caption) = attrs.caption {
            wnd.gtk_wnd.set_title(&caption);
        }

        if let Some(visible) = attrs.visible {
            if visible {
                wnd.gtk_wnd.show_all();
            } else {
                wnd.gtk_wnd.hide();
            }
        }

        // Unborrow `WNDS` before dropping `old_listener` (which might execute
        // user code)
        drop(wnds);
    }

    /// Implements `Wm::remove_wnd`.
    pub(super) fn remove_wnd(&self, wm: Wm) {
        let wnd = WNDS
            .get_with_wm(wm)
            .borrow_mut()
            .deallocate(self.ptr)
            .unwrap();

        // Delete scroll tick callback
        if let Some(scroll_state) = &wnd.scroll_state {
            if let Some(momentum_state) = &scroll_state.momentum {
                unsafe {
                    gtk_sys::gtk_widget_remove_tick_callback(
                        wnd.gtk_widget.upcast_ref::<gtk::Widget>().as_ptr(),
                        momentum_state.tick_callback_id,
                    );
                }
            }
        }

        // Suppress further callbacks
        wnd.gtk_widget.wnd_ptr().set(None);

        // Destroy the window
        wnd.gtk_wnd.destroy();

        COMPOSITOR
            .get_with_wm(wm)
            .borrow_mut()
            .remove_wnd(&wnd.comp_wnd);
    }

    /// Implements `Wm::update_wnd`.
    pub(super) fn update_wnd(&self, wm: Wm) {
        if DRAWING_WND.get_with_wm(wm).get() == Some(self.ptr) {
            return;
        }

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let wnd = &mut wnds[self.ptr];

        if !wnd.gtk_wnd.is_visible() {
            return;
        }

        let (surf_size, dpi_scale) = comp_surf_props_for_widget(&wnd.gtk_widget);

        let added_dirty_rect = COMPOSITOR.get_with_wm(wm).borrow_mut().update_wnd(
            &mut wnd.comp_wnd,
            surf_size,
            dpi_scale,
        );

        if let Some(r) = added_dirty_rect {
            let fac = wnd.gtk_wnd.get_scale_factor();
            let x = r.min.x as i32 / fac;
            let y = r.min.y as i32 / fac;
            let width = (r.max.x as i32 + fac - 1) / fac - x;
            let height = (r.max.y as i32 + fac - 1) / fac - y;
            wnd.gtk_widget.queue_draw_area(x, y, width, height);
        }
    }

    /// Implements `Wm::get_wnd_size`.
    pub(super) fn get_wnd_size(&self, wm: Wm) -> [u32; 2] {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let gtk_widget = &wnds[self.ptr].gtk_widget;
        [
            gtk_widget.get_allocated_width() as u32,
            gtk_widget.get_allocated_height() as u32,
        ]
    }

    /// Implements `Wm::get_wnd_dpi_scale`.
    pub(super) fn get_wnd_dpi_scale(&self, wm: Wm) -> f32 {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let gtk_wnd = &wnds[self.ptr].gtk_wnd;
        gtk_wnd.get_scale_factor() as f32
    }

    // Implements `Wm::is_wnd_focused`.
    pub(super) fn is_wnd_focused(&self, wm: Wm) -> bool {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let gtk_wnd = &wnds[self.ptr].gtk_wnd;
        !gtk_wnd
            .get_state_flags()
            .contains(gtk::StateFlags::BACKDROP)
    }

    /// Implements `Wm::request_update_ready_wnd`.
    pub(super) fn request_update_ready_wnd(&self, wm: Wm) {
        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let wnd = &mut wnds[self.ptr];

        // If we are currently inside a tick handler, tell the handler
        // not to stop the tick callback
        wnd.tick_callback_continue = true;

        if !wnd.tick_callback_active {
            wnd.tick_callback_active = true;
            unsafe {
                gtk_sys::gtk_widget_add_tick_callback(
                    wnd.gtk_widget.upcast_ref::<gtk::Widget>().as_ptr(),
                    Some(Self::handle_tick_callback),
                    self.ptr.into_raw().as_ptr() as _,
                    None,
                );
            }
        }
    }

    extern "C" fn handle_tick_callback(
        _: *mut gtk_sys::GtkWidget,
        _: *mut gdk_sys::GdkFrameClock,
        userdata: glib_sys::gpointer,
    ) -> glib_sys::gboolean {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr: WndPoolPtr = unsafe { PoolPtr::from_raw(NonNull::new_unchecked(userdata as _)) };
        let hwnd = HWnd { ptr };

        let listener = {
            let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
            let wnd = if let Some(wnd) = wnds.get_mut(ptr) {
                wnd
            } else {
                // The window is gone
                return glib_sys::G_SOURCE_REMOVE;
            };
            debug_assert!(wnd.tick_callback_active);
            wnd.tick_callback_continue = false;
            Rc::clone(&wnd.listener)
        };

        listener.update_ready(wm, &hwnd);

        // Decide whether we should stop the tick callback or not
        let cont = {
            let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
            let wnd = if let Some(wnd) = wnds.get_mut(ptr) {
                wnd
            } else {
                // The window was removed by `listener.update_ready`
                return glib_sys::G_SOURCE_REMOVE;
            };
            if wnd.tick_callback_continue {
                true
            } else {
                wnd.tick_callback_active = false;
                false
            }
        };

        if cont {
            glib_sys::G_SOURCE_CONTINUE
        } else {
            glib_sys::G_SOURCE_REMOVE
        }
    }

    pub(super) fn gtk_window(&self, wm: Wm) -> gtk::Window {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let wnd = &wnds[self.ptr];
        wnd.gtk_wnd.clone()
    }

    pub(super) fn gdk_window(&self, wm: Wm) -> Option<gdk::Window> {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let wnd = &wnds[self.ptr];
        wnd.gtk_wnd.get_window()
    }

    pub(super) fn set_im_ctx_active(
        &self,
        wm: Wm,
        im_ctx: &impl IsA<gtk::IMContext>,
        active: bool,
    ) {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let wnd = &wnds[self.ptr];
        wnd.gtk_widget.set_im_ctx_active(im_ctx, active);
    }
}

fn comp_surf_props_for_widget(w: &WndWidget) -> ([usize; 2], f32) {
    let factor = w.get_scale_factor() as usize;

    (
        [
            w.get_allocated_width() as usize * factor,
            w.get_allocated_height() as usize * factor,
        ],
        factor as f32,
    )
}

/// Used by `TcwWndWidget`'s callback functions. Mutably borrow `WNDS` and
/// call the given closure with `Wnd`, `HWnd`, and `Wm`.
fn with_wnd_mut<R>(wm: Wm, wnd_ptr: WndPtr, f: impl FnOnce(&mut Wnd, HWnd, Wm) -> R) -> Option<R> {
    let ptr = wnd_ptr?;

    let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
    let wnd = wnds.get_mut(ptr)?;
    Some(f(wnd, HWnd { ptr }, wm))
}

/// Handles `GtkWidgetClass::draw`. `wnd_ptr` is retrieved from
/// `TcwWndWidget::wnd_ptr`.
#[no_mangle]
extern "C" fn tcw_wnd_widget_draw_handler(wnd_ptr: WndPtr, cairo_ctx: *mut cairo_sys::cairo_t) {
    // Emit `resize` event if needed. `resize`'s event handler may call
    // `Wm::update_wnd`.
    if let Some(Some((wm, hwnd, listener))) = with_wnd_mut(
        unsafe { Wm::global_unchecked() },
        wnd_ptr,
        |wnd, hwnd, wm| {
            let size = [
                wnd.gtk_wnd.get_allocated_width(),
                wnd.gtk_wnd.get_allocated_height(),
            ];
            if size != wnd.size {
                wnd.size = size;
                Some((wm, hwnd, Rc::clone(&wnd.listener)))
            } else {
                None
            }
        },
    ) {
        // Suppress `Wm::update_wnd`
        DRAWING_WND.get_with_wm(wm).set(Some(hwnd.ptr));

        listener.resize(wm, &hwnd);

        DRAWING_WND.get_with_wm(wm).set(None);
    }

    with_wnd_mut(unsafe { Wm::global_unchecked() }, wnd_ptr, |wnd, _, wm| {
        let mut compositor = COMPOSITOR.get_with_wm(wm).borrow_mut();

        let (surf_size, dpi_scale) = comp_surf_props_for_widget(&wnd.gtk_widget);
        compositor.update_wnd(&mut wnd.comp_wnd, surf_size, dpi_scale);

        compositor.paint_wnd(&mut wnd.comp_wnd);

        let cr = unsafe { cairo::Context::from_glib_borrow(cairo_ctx) };
        if let Some(surface) = wnd.comp_wnd.cairo_surface() {
            cr.set_source_surface(surface, 0.0, 0.0);
            cr.set_operator(cairo::Operator::Source);
            cr.paint();
        }
    });
}

/// Handles `notify::scale-factor`.
#[no_mangle]
extern "C" fn tcw_wnd_widget_dpi_scale_changed_handler(wnd_ptr: WndPtr) {
    if let Some((wm, hwnd, listener)) = with_wnd_mut(
        unsafe { Wm::global_unchecked() },
        wnd_ptr,
        |wnd, hwnd, wm| (wm, hwnd, Rc::clone(&wnd.listener)),
    ) {
        listener.dpi_scale_changed(wm, &hwnd);
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_nc_hit_test_handler(wnd_ptr: WndPtr, x: f32, y: f32) -> c_int {
    log::debug!("nc_hit_test{:?}", (wnd_ptr, x, y));

    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let loc = Point2::new(x, y);

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();

        // Stop any ongoing scroll gesture (just in case)
        wnds = stop_scroll(wm, wnds, hwnd.clone());

        let wnd = wnds.get_mut(ptr)?;

        if wnd.drag_state.is_some() {
            // There already is an active drag gesture
            return Some(0);
        }

        // Unborrow `WNDS` before calling into user code
        let listener = Rc::clone(&wnd.listener);
        drop(wnds);

        let hit = listener.nc_hit_test(wm, &hwnd, loc);

        Some(match hit {
            iface::NcHit::Client => 0,
            iface::NcHit::Grab => 1,
        })
    })()
    .unwrap_or(0)
}

struct EnumAccel<F: FnMut(&AccelTable)>(F);

impl<F: FnMut(&AccelTable)> iface::InterpretEventCtx<AccelTable> for EnumAccel<F> {
    fn use_accel(&mut self, accel: &AccelTable) {
        (self.0)(accel);
    }
}

struct KeyEvent {
    keyval: u32,
    mod_flags: u8,
}

impl iface::KeyEvent<AccelTable> for KeyEvent {
    fn translate_accel(&self, accel_table: &AccelTable) -> Option<iface::ActionId> {
        accel_table.find_action_with_key(self.keyval, self.mod_flags)
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_key_press_handler(
    wnd_ptr: WndPtr,
    event: *mut gdk_sys::GdkEventKey,
) -> c_int {
    let event = unsafe { gdk::EventKey::from_glib_borrow(event) };

    log::debug!(
        "key_press{:?}",
        (wnd_ptr, event.get_keyval(), event.get_state())
    );

    if let Some((wm, hwnd, listener, is_im_ctx_active)) = with_wnd_mut(
        unsafe { Wm::global_unchecked() },
        wnd_ptr,
        |wnd, hwnd, wm| {
            (
                wm,
                hwnd,
                Rc::clone(&wnd.listener),
                wnd.gtk_widget.is_im_ctx_active(),
            )
        },
    ) {
        let mut action = None;
        let action_ref = &mut action;
        let keyval = event.get_keyval();
        let mod_flags = AccelTable::compress_mod_flags(event.get_state().bits());

        let mut interpret_event_ctx = EnumAccel(move |accel_table| {
            if action_ref.is_none() {
                *action_ref = accel_table.find_action_with_key(keyval, mod_flags);
            }
        });
        listener.interpret_event(wm, &hwnd, &mut interpret_event_ctx);

        // Interpret text input actions. Do this after calling `interpret_event`
        // so that they can be shadowed by custom accelerator tables.
        if is_im_ctx_active {
            iface::InterpretEventCtx::use_accel(&mut interpret_event_ctx, &TEXT_INPUT_ACCEL);
        }

        log::trace!("... action = {:?}", action);

        if let Some(action) = action {
            // The action was found. Can the window handle it?
            let status = listener.validate_action(wm, &hwnd, action);
            if status.contains(iface::ActionStatus::VALID) {
                if status.contains(iface::ActionStatus::ENABLED) {
                    listener.perform_action(wm, &hwnd, action);
                    return 1; // Handled
                }
                return 0;
            }
        }

        let handled = listener.key_down(wm, &hwnd, &KeyEvent { keyval, mod_flags });
        log::trace!("... key_down(...) = {:?}", handled);

        handled as _
    } else {
        0
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_key_release_handler(
    wnd_ptr: WndPtr,
    event: *mut gdk_sys::GdkEventKey,
) -> c_int {
    let event = unsafe { gdk::EventKey::from_glib_borrow(event) };

    log::debug!(
        "key_release{:?}",
        (wnd_ptr, event.get_keyval(), event.get_state())
    );

    if let Some((wm, hwnd, listener)) = with_wnd_mut(
        unsafe { Wm::global_unchecked() },
        wnd_ptr,
        |wnd, hwnd, wm| (wm, hwnd, Rc::clone(&wnd.listener)),
    ) {
        let keyval = event.get_keyval();
        let mod_flags = AccelTable::compress_mod_flags(event.get_state().bits());

        let handled = listener.key_up(wm, &hwnd, &KeyEvent { keyval, mod_flags });
        log::trace!("... key_up(...) = {:?}", handled);

        handled as _
    } else {
        0
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_button_handler(
    wnd_ptr: WndPtr,
    x: f32,
    y: f32,
    is_pressed: c_int,
    button: c_int,
) {
    log::debug!("button{:?}", (wnd_ptr, x, y, is_pressed != 0, button));
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let loc = Point2::new(x, y);
        let button_mask = 1 << button;

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();

        // Stop any ongoing scroll gesture (just in case)
        wnds = stop_scroll(wm, wnds, hwnd.clone());

        let mut wnd = wnds.get_mut(ptr)?;

        if is_pressed != 0 {
            // Mouse button pressed
            let drag_state = if let Some(drag_state) = &mut wnd.drag_state {
                drag_state
            } else {
                // Unborrow `WNDS` before calling into user code
                let listener = Rc::clone(&wnd.listener);
                drop(wnds);

                // Create `MouseDragState`
                let drag_state = MouseDragState {
                    listener: listener.mouse_drag(wm, &hwnd, loc, button as u8).into(),
                    pressed_buttons: 0,
                };

                // Re-borrow `WNDS` and set `drag_state`
                wnds = WNDS.get_with_wm(wm).borrow_mut();
                wnd = wnds.get_mut(ptr)?;
                debug_assert!(wnd.drag_state.is_none());
                wnd.drag_state = Some(drag_state);
                wnd.drag_state.as_mut().unwrap()
            };

            if (drag_state.pressed_buttons & button_mask) != 0 {
                return None;
            }
            drag_state.pressed_buttons |= button_mask;

            // Call `MouseDragListener::mouse_down`
            let drag_listener = Rc::clone(&drag_state.listener);

            drop(wnds);
            drag_listener.mouse_down(wm, &hwnd, loc, button as u8);
        } else {
            // Mouse button released
            let drag_state = wnd.drag_state.as_mut()?;

            if (drag_state.pressed_buttons & button_mask) == 0 {
                return None;
            }
            drag_state.pressed_buttons &= !button_mask;

            let drag_listener = if drag_state.pressed_buttons == 0 {
                // Remove `MouseDragState` from `Wnd`
                wnd.drag_state.take().unwrap().listener
            } else {
                Rc::clone(&drag_state.listener)
            };

            // Call `MouseDragListener::mouse_up`
            drop(wnds);
            drag_listener.mouse_up(wm, &hwnd, loc, button as u8);
        }

        Some(())
    })();
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_motion_handler(wnd_ptr: WndPtr, x: f32, y: f32) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let loc = Point2::new(x, y);

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();

        // Stop any ongoing scroll gesture (just in case)
        wnds = stop_scroll(wm, wnds, hwnd.clone());

        let wnd = wnds.get(ptr)?;

        if let Some(drag_state) = wnd.drag_state.as_ref() {
            // `MouseDragListener::mouse_motion`
            let listener = Rc::clone(&drag_state.listener);

            drop(wnds);
            listener.mouse_motion(wm, &hwnd, loc);
        } else {
            // `WndListener::mouse_motion`
            let listener = Rc::clone(&wnd.listener);

            drop(wnds);
            listener.mouse_motion(wm, &hwnd, loc);
        }

        Some(())
    })();
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_leave_handler(wnd_ptr: WndPtr) {
    log::debug!("leave{:?}", (wnd_ptr,));
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let wnd = wnds.get_mut(ptr)?;

        let listener = Rc::clone(&wnd.listener);

        drop(wnds);
        listener.mouse_leave(wm, &hwnd);

        Some(())
    })();
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_discrete_scroll_handler(
    wnd_ptr: WndPtr,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();

        // Stop any ongoing scroll gesture (just in case)
        wnds = stop_scroll(wm, wnds, hwnd.clone());

        let wnd = wnds.get(ptr)?;

        let listener = Rc::clone(&wnd.listener);
        drop(wnds);
        listener.scroll_motion(
            wm,
            &hwnd,
            [x, y].into(),
            &iface::ScrollDelta {
                delta: [delta_x, delta_y].into(),
                precise: false,
            },
        );

        Some(())
    });
}

impl ScrollState {
    /// Return an event in `self.history`. `i = 1` represents the last event.
    fn past_event(&self, i: usize) -> &ScrollEvent {
        debug_assert!(i > 0);
        &self.history[(self.history_index.0 as usize).wrapping_sub(i) % SCROLL_HISTORY_LEN]
    }

    /// Estimate the scroll velocity based on recent event recrods.
    fn velocity(&self, time: Wrapping<u32>) -> Vector2<f32> {
        let mut earliest_time = time;
        let mut num_events = 0;

        while num_events < SCROLL_HISTORY_LEN {
            let e = self.past_event(num_events + 1);
            let delta = earliest_time - e.time;
            if delta.0 > 50 {
                // Too distant, probably a separate series of events
                break;
            }
            earliest_time = e.time;
            num_events += 1;
        }

        // Needs at least two events to estimate the velocity
        if num_events >= 2 {
            let latest_time = self.past_event(1).time;
            //
            //      ───────────────→ time
            //   delta:   3   2   1     (each number represents the event
            //                           wherein the delta value is recorded)
            //       k:     3   2   1   (numEvents = 3)
            //              ↑       ↑
            //              │       └─ event.timestamp
            //              └───────── timestamp
            //
            // In this example, the delta values from the two events 1 and 2
            // should be summed and divided by the timing difference between the
            // events 1 and 3.

            let sum: Vector2<f32> = (1..num_events).map(|i| self.past_event(i).delta).sum();
            sum * (1000.0 / (latest_time - earliest_time).0 as f32)
        } else {
            [0.0, 0.0].into()
        }
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_smooth_scroll_handler(
    wnd_ptr: WndPtr,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
    time: Wrapping<u32>,
) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let loc = Point2::new(x, y);
        let delta = iface::ScrollDelta {
            delta: -Vector2::new(delta_x, delta_y),
            precise: false,
        };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();

        // Preempt momentum scrollng
        wnds = stop_momentum_scroll(wm, wnds, hwnd.clone());

        let mut wnd = wnds.get_mut(ptr)?;

        let scroll_state = if let Some(scroll_state) = &mut wnd.scroll_state {
            scroll_state
        } else {
            // Unborrow `WNDS` before calling into user code
            let listener = Rc::clone(&wnd.listener);
            drop(wnds);

            // Create `ScrollState`
            let scroll_state = ScrollState {
                listener: listener.scroll_gesture(wm, &hwnd, loc).into(),
                history: [ScrollEvent {
                    delta: [0.0, 0.0].into(),
                    time: time - Wrapping(0x80000000), // 20 days earlier
                }; SCROLL_HISTORY_LEN],
                history_index: Wrapping(0),
                momentum: None,
            };

            // Re-borrow `WNDS` and set `scroll_state`
            wnds = WNDS.get_with_wm(wm).borrow_mut();
            wnd = wnds.get_mut(ptr)?;
            debug_assert!(wnd.scroll_state.is_none());
            wnd.scroll_state = Some(scroll_state);
            wnd.scroll_state.as_mut().unwrap()
        };

        scroll_state.history[scroll_state.history_index.0 as usize % SCROLL_HISTORY_LEN] =
            ScrollEvent {
                delta: delta.delta,
                time,
            };
        scroll_state.history_index += Wrapping(1u8);

        let velocity = scroll_state.velocity(time);

        // Call `ScrollListener::motion`
        let scroll_listener = Rc::clone(&scroll_state.listener);

        drop(wnds);
        scroll_listener.motion(wm, &hwnd, &delta, velocity);

        Some(())
    })();
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_smooth_scroll_stop_handler(wnd_ptr: WndPtr, time: Wrapping<u32>) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = wnd_ptr?;
        let hwnd = HWnd { ptr };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();

        // Preempt (existing) momentum scrollng
        wnds = stop_momentum_scroll(wm, wnds, hwnd.clone());

        let wnd = wnds.get_mut(ptr)?;

        let scroll_state = wnd.scroll_state.as_mut()?;
        let velocity = scroll_state.velocity(time);

        if velocity != Vector2::new(0.0, 0.0) {
            // Start a momentum phase
            let tick_callback_id = unsafe {
                gtk_sys::gtk_widget_add_tick_callback(
                    wnd.gtk_widget.upcast_ref::<gtk::Widget>().as_ptr(),
                    Some(handle_momentum_scroll_tick_callback),
                    ptr.into_raw().as_ptr() as _,
                    None,
                )
            };

            scroll_state.momentum = Some(MomentumScrollState {
                tick_callback_id,
                velocity,
                last_frame_time: -1,
                elapsed_time: 0,
            });

            // Call `ScrollListener::start_momentum_phase`. But, before calling into user code,
            // we should unborrow `WNDS`.
            let listener = Rc::clone(&scroll_state.listener);
            drop(wnds);
            listener.start_momentum_phase(wm, &hwnd);
        } else {
            let scroll_state = wnd.scroll_state.take().unwrap();

            // Call `ScrollListener::end`. But, before calling into user code,
            // we should unborrow `WNDS`.
            drop(wnds);
            scroll_state.listener.end(wm, &hwnd);
        }

        Some(())
    })();
}

extern "C" fn handle_momentum_scroll_tick_callback(
    _: *mut gtk_sys::GtkWidget,
    frame_clock: *mut gdk_sys::GdkFrameClock,
    userdata: glib_sys::gpointer,
) -> glib_sys::gboolean {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = unsafe { PoolPtr::from_raw(NonNull::new(userdata as _).unwrap()) };
        let hwnd = HWnd { ptr };

        let frame_clock = unsafe { gdk::FrameClock::from_glib_borrow(frame_clock) };
        let frame_time = frame_clock.get_frame_time();

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let wnd = wnds.get_mut(ptr)?;

        let scroll_state = wnd.scroll_state.as_mut()?;
        let momentum_state = scroll_state.momentum.as_mut()?;

        // Evaluate the animation
        let (decel_curve1, _) = eval_deceleration(momentum_state.elapsed_time);

        if momentum_state.last_frame_time != -1 {
            // Update the elapsed time. Convert microseconds to milliseconds
            // by shifting by 10 bits.
            momentum_state.elapsed_time +=
                ((frame_time >> 10) - (momentum_state.last_frame_time >> 10)) as u32;
        }
        momentum_state.last_frame_time = frame_time;

        let (decel_curve2, decel_curve_vel) = eval_deceleration(momentum_state.elapsed_time);

        let end = momentum_state.elapsed_time >= MOMENTUM_DURATION;

        // Calculate the delta
        let delta = momentum_state.velocity * (decel_curve2 - decel_curve1);
        let velocity = momentum_state.velocity * decel_curve_vel;

        // Grab the listener, and remove the `ScrollState`
        // if the animation is done
        let scroll_listener = Rc::clone(&scroll_state.listener);

        if end {
            wnd.scroll_state = None;
        }

        // Call handlers
        drop(wnds);

        scroll_listener.motion(
            wm,
            &hwnd,
            &iface::ScrollDelta {
                delta,
                precise: false,
            },
            velocity,
        );

        if end {
            scroll_listener.end(wm, &hwnd);
            Some(glib_sys::G_SOURCE_REMOVE)
        } else {
            Some(glib_sys::G_SOURCE_CONTINUE)
        }
    })()
    .unwrap_or(glib_sys::G_SOURCE_REMOVE)
}

/// Evaluate the deceleration animation at time `t` (measured in milliseconds).
///
/// Returns two values: `f(t)` and `f'(t)`.
fn eval_deceleration(t: u32) -> (f32, f32) {
    // Let T = MOMENTUM_DURATION / 1000.
    // We need a smooth function f such that f(0) = 0, f'(0) = 1,
    // and f'(T) = 0. We define f as:
    //     f(t) = (1 - (1 - t / T)²) * T/2
    // The derivative is:
    //     f'(t) = 1 - t / T
    if t < MOMENTUM_DURATION {
        let p = (t as f32) * (1.0 / MOMENTUM_DURATION as f32);
        (
            (MOMENTUM_DURATION as f32 / 2000.0) * (1.0 - (1.0 - p) * (1.0 - p)),
            1.0 - p,
        )
    } else {
        (MOMENTUM_DURATION as f32 / 2000.0, 0.0)
    }
}

/// Abort an ongoing scroll gesture if it's currently in the momentum phase.
fn stop_momentum_scroll(wm: Wm, wnds: RefMut<'_, WndPool>, hwnd: HWnd) -> RefMut<'_, WndPool> {
    if let Some(wnd) = wnds.get(hwnd.ptr) {
        if let Some(scroll_state) = &wnd.scroll_state {
            if scroll_state.momentum.is_some() {
                return stop_scroll(wm, wnds, hwnd);
            }
        }
    }

    wnds
}

/// Abort an ongoing scroll gesture.
fn stop_scroll(wm: Wm, mut wnds: RefMut<'_, WndPool>, hwnd: HWnd) -> RefMut<'_, WndPool> {
    if let Some(wnd) = wnds.get_mut(hwnd.ptr) {
        if let Some(scroll_state) = wnd.scroll_state.take() {
            if let Some(momentum_state) = &scroll_state.momentum {
                unsafe {
                    gtk_sys::gtk_widget_remove_tick_callback(
                        wnd.gtk_widget.upcast_ref::<gtk::Widget>().as_ptr(),
                        momentum_state.tick_callback_id,
                    );
                }
            }

            // Unborrow `WNDS` before calling `end` and dropping the listener
            drop(wnds);

            scroll_state.listener.end(wm, &hwnd);
            drop(scroll_state);

            // Reborrow `WNDS`
            return WNDS.get_with_wm(wm).borrow_mut();
        }
    }

    wnds
}

// ============================================================================
// Accelerator tables
//
// Most of these types are implementation details and thus hidden. They still
// need to be `pub` because they are instantiated by `accel_table!`.

#[derive(Debug)]
pub struct AccelTable {
    #[doc(hidden)]
    pub key: &'static [ActionKeyBinding],
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ActionKeyBinding {
    pub action: iface::ActionId,
    pub mod_flags: u8,
    pub keyval: c_uint,
}

impl AccelTable {
    const fn compress_mod_flags(x: u32) -> u8 {
        // let SHIFT, CONTROL, SUPER, and META pass through
        ((x & 0b101) | ((x >> 23) & 0b101000)) as u8
    }

    pub const MOD_SHIFT: u8 = Self::compress_mod_flags(gdk_sys::GDK_SHIFT_MASK);
    pub const MOD_CONTROL: u8 = Self::compress_mod_flags(gdk_sys::GDK_CONTROL_MASK);
    pub const MOD_SUPER: u8 = Self::compress_mod_flags(gdk_sys::GDK_SUPER_MASK);
    pub const MOD_META: u8 = Self::compress_mod_flags(gdk_sys::GDK_META_MASK);

    fn find_action_with_key(&self, keyval: u32, mod_flags: u8) -> Option<iface::ActionId> {
        self.key
            .iter()
            .filter(move |binding| mod_flags == binding.mod_flags && keyval == binding.keyval)
            .map(|binding| binding.action)
            .nth(0)
    }
}

static TEXT_INPUT_ACCEL: AccelTable = tcw3_pal_macro::accel_table_inner!(
    crate,
    "gtk",
    [
        (actions::DELETE_BACKWARD, gtk("Backspace")),
        (actions::DELETE_BACKWARD_WORD, gtk("Ctrl+Backspace")),
        (actions::DELETE_FORWARD, gtk("Delete")),
        (actions::DELETE_FORWARD_WORD, gtk("Ctrl+Delete")),
        (actions::INSERT_LINE_BREAK, gtk("Shift+Return")),
        (actions::INSERT_PARAGRAPH_BREAK, gtk("Return")),
        (actions::INSERT_TAB, gtk("Tab")),
        (actions::INSERT_BACKTAB, gtk("Shift+Tab")),
        (actions::MOVE_LEFT, gtk("Left")),
        (actions::MOVE_RIGHT, gtk("Right")),
        (actions::MOVE_LEFT_WORD, gtk("Ctrl+Left")),
        (actions::MOVE_RIGHT_WORD, gtk("Ctrl+Right")),
        (actions::MOVE_START_OF_LINE, gtk("Home")),
        (actions::MOVE_END_OF_LINE, gtk("End")),
        (actions::MOVE_START_OF_PARAGRAPH, gtk("Ctrl+Up")),
        (actions::MOVE_END_OF_PARAGRAPH, gtk("Ctrl+Down")),
        (actions::MOVE_START_OF_DOCUMENT, gtk("Ctrl+Home")),
        (actions::MOVE_END_OF_DOCUMENT, gtk("Ctrl+End")),
        (actions::MOVE_UP, gtk("Up")),
        (actions::MOVE_DOWN, gtk("Down")),
        (actions::MOVE_UP_PAGE, gtk("PageUp")),
        (actions::MOVE_DOWN_PAGE, gtk("PageDown")),
        (actions::MOVE_LEFT_SELECTING, gtk("Shift+Left")),
        (actions::MOVE_RIGHT_SELECTING, gtk("Shift+Right")),
        (actions::MOVE_LEFT_WORD_SELECTING, gtk("Shift+Ctrl+Left")),
        (actions::MOVE_RIGHT_WORD_SELECTING, gtk("Shift+Ctrl+Right")),
        (actions::MOVE_START_OF_LINE_SELECTING, gtk("Shift+Home")),
        (actions::MOVE_END_OF_LINE_SELECTING, gtk("Shift+End")),
        (
            actions::MOVE_START_OF_PARAGRAPH_SELECTING,
            gtk("Shift+Ctrl+Up")
        ),
        (
            actions::MOVE_END_OF_PARAGRAPH_SELECTING,
            gtk("Shift+Ctrl+Down")
        ),
        (
            actions::MOVE_START_OF_DOCUMENT_SELECTING,
            gtk("Shift+Ctrl+Home")
        ),
        (
            actions::MOVE_END_OF_DOCUMENT_SELECTING,
            gtk("Shift+Ctrl+End")
        ),
        (actions::MOVE_UP_SELECTING, gtk("Shift+Up")),
        (actions::MOVE_DOWN_SELECTING, gtk("Shift+Down")),
        (actions::MOVE_UP_PAGE_SELECTING, gtk("Shift+PageUp")),
        (actions::MOVE_DOWN_PAGE_SELECTING, gtk("Shift+PageDown")),
    ]
);

// ============================================================================

// These type are not actually `pub`, but `pub` is required by `glib_wrapper!`
glib_wrapper! {
    /// `TcwWndWidget` is defined in `wndwidget.c`.
    pub struct WndWidget(Object<TcwWndWidget, TcwWndWidgetClass, WndWidgetClass>)
        @extends gtk::Widget;

    match fn {
        get_type => || tcw_wnd_widget_get_type(),
    }
}

extern "C" {
    fn tcw_wnd_widget_new() -> *mut gtk_sys::GtkWidget;
    fn tcw_wnd_widget_get_type() -> glib_sys::GType;
}

// These types are defined in `wndwidget.c`
#[repr(C)]
pub struct TcwWndWidget {
    parent_instance: gtk_sys::GtkDrawingArea,
    /// Stores `HWnd`. This field is only touched by a main thread, so it's safe
    /// to access through `Cell`.
    wnd_ptr: Cell<WndPtr>,
    im_ctx: Cell<*mut gtk_sys::GtkIMContext>,
}

#[repr(C)]
#[derive(Debug)]
pub struct TcwWndWidgetClass {
    parent_class: gtk_sys::GtkDrawingAreaClass,
}

type WndPtr = Option<WndPoolPtr>;

impl WndWidget {
    fn new(_: Wm) -> Self {
        // We have `Wm`, so we know we are on the main thread, hence this is safe
        unsafe { gtk::Widget::from_glib_none(tcw_wnd_widget_new()).unsafe_cast() }
    }

    fn wnd_ptr(&self) -> &Cell<WndPtr> {
        unsafe { &(*self.as_ptr()).wnd_ptr }
    }

    fn set_im_ctx_active(&self, im_ctx: &impl IsA<gtk::IMContext>, active: bool) {
        unsafe {
            let this = &*self.as_ptr();

            let cur = this.im_ctx.get();
            let new = im_ctx.as_ptr() as *mut gtk_sys::GtkIMContext;

            // - `cur == new && active`: `new` is already active, so this is
            //   no-op
            // - `cur == new && !active`: `new` should be deactivated
            // - `cur != new && active`: `cur` should be deactivated and `new`
            //   should be activated instead
            // - `cur != new && !active`: `new` is already inactive, so this is
            //   no-op
            if active || cur == new {
                this.im_ctx.set(null_mut());
                if !cur.is_null() {
                    gobject_sys::g_object_unref(cur as _);
                }
            }

            if active {
                this.im_ctx.set(new);
                gobject_sys::g_object_ref(new as _);
            }
        }
    }

    fn is_im_ctx_active(&self) -> bool {
        unsafe {
            let this = &*self.as_ptr();
            !this.im_ctx.get().is_null()
        }
    }
}
