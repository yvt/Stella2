use cgmath::Point2;
use glib::{
    glib_object_wrapper, glib_wrapper,
    translate::{FromGlibPtrBorrow, FromGlibPtrFull, FromGlibPtrNone, ToGlibPtr},
};
use gtk::prelude::*;
use iterpool::{Pool, PoolPtr};
use std::{
    cell::{Cell, RefCell},
    num::NonZeroUsize,
    os::raw::c_int,
    rc::Rc,
};

use super::{comp, Wm, WndAttrs};
use crate::{cells::MtLazyStatic, iface, iface::Wm as WmTrait, mt_lazy_static, MtSticky};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd {
    ptr: PoolPtr,
}

static WNDS: MtSticky<RefCell<Pool<Wnd>>, Wm> = {
    // `Wnd` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(Pool::new())) }
};

mt_lazy_static! {
    pub(super) static <Wm> ref COMPOSITOR: RefCell<comp::Compositor> =>
        |_| RefCell::new(comp::Compositor::new());
}

static DRAWING_WND: MtSticky<Cell<Option<PoolPtr>>, Wm> = MtSticky::new(Cell::new(None));

struct Wnd {
    gtk_wnd: gtk::Window,
    gtk_widget: WndWidget,
    comp_wnd: comp::Wnd,
    // TODO: Handle the following events:
    //       - scroll_motion
    //       - scroll_gesture
    listener: Rc<dyn iface::WndListener<Wm>>,

    /// The last known size of the window.
    size: [i32; 2],

    tick_callback_active: bool,
    tick_callback_continue: bool,

    drag_state: Option<MouseDragState>,
}

struct MouseDragState {
    listener: Rc<dyn iface::MouseDragListener<Wm>>,
    pressed_buttons: u32,
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
            listener: Rc::new(()),
            size: [0, 0],
            tick_callback_active: false,
            tick_callback_continue: false,
            drag_state: None,
        };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let ptr = wnds.allocate(wnd);

        // Connect window events
        let wnd = &wnds[ptr];
        wnd.gtk_widget.wnd_ptr().set(ptr.0.get());

        wnd.gtk_wnd.connect_delete_event(move |_, _| {
            let listener = {
                let wnds = WNDS.get_with_wm(wm).borrow();
                Rc::clone(&wnds[ptr].listener)
            };

            listener.close_requested(wm, &Self { ptr });

            Inhibit(true)
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
        // TODO: cursor_shape

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

        // Suppress further callbacks
        wnd.gtk_widget.wnd_ptr().set(0);

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
            true,
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
                    self.ptr.0.get() as _,
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
        let ptr = PoolPtr(NonZeroUsize::new(userdata as _).unwrap());
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
fn with_wnd_mut<R>(wm: Wm, wnd_ptr: usize, f: impl FnOnce(&mut Wnd, HWnd, Wm) -> R) -> Option<R> {
    let ptr = PoolPtr(NonZeroUsize::new(wnd_ptr)?);

    let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
    let wnd = wnds.get_mut(ptr)?;
    Some(f(wnd, HWnd { ptr }, wm))
}

/// Handles `GtkWidgetClass::draw`. `wnd_ptr` is retrieved from
/// `TcwWndWidget::wnd_ptr`.
#[no_mangle]
extern "C" fn tcw_wnd_widget_draw_handler(wnd_ptr: usize, cairo_ctx: *mut cairo_sys::cairo_t) {
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
        compositor.update_wnd(&mut wnd.comp_wnd, surf_size, dpi_scale, false);

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
extern "C" fn tcw_wnd_widget_dpi_scale_changed_handler(wnd_ptr: usize) {
    if let Some((wm, hwnd, listener)) = with_wnd_mut(
        unsafe { Wm::global_unchecked() },
        wnd_ptr,
        |wnd, hwnd, wm| (wm, hwnd, Rc::clone(&wnd.listener)),
    ) {
        listener.dpi_scale_changed(wm, &hwnd);
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_button_handler(
    wnd_ptr: usize,
    x: f32,
    y: f32,
    is_pressed: c_int,
    button: c_int,
) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = PoolPtr(NonZeroUsize::new(wnd_ptr)?);
        let hwnd = HWnd { ptr };

        let loc = Point2::new(x, y);
        let button_mask = 1 << button;

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let mut wnd = wnds.get_mut(ptr)?;

        if is_pressed != 0 {
            // Mouse button pressed
            let drag_state = if let Some(drag_state) = &mut wnd.drag_state {
                drag_state
            } else {
                // Unborrow `WNDS` before calling into user code
                let listener = Rc::clone(&wnd.listener);
                drop(wnd);
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
extern "C" fn tcw_wnd_widget_motion_handler(wnd_ptr: usize, x: f32, y: f32) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = PoolPtr(NonZeroUsize::new(wnd_ptr)?);
        let hwnd = HWnd { ptr };

        let loc = Point2::new(x, y);

        let wnds = WNDS.get_with_wm(wm).borrow_mut();
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
extern "C" fn tcw_wnd_widget_leave_handler(wnd_ptr: usize) {
    (|| {
        let wm = unsafe { Wm::global_unchecked() };
        let ptr = PoolPtr(NonZeroUsize::new(wnd_ptr)?);
        let hwnd = HWnd { ptr };

        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let mut wnd = wnds.get_mut(ptr)?;

        if let Some(drag_state) = wnd.drag_state.take() {
            // Cancel the mouse drag gesture first
            let listener = Rc::clone(&drag_state.listener);

            drop(wnds);
            listener.cancel(wm, &hwnd);

            // Re-borrow `WNDS`
            wnds = WNDS.get_with_wm(wm).borrow_mut();
            wnd = wnds.get_mut(ptr)?;
        }

        let listener = Rc::clone(&wnd.listener);

        drop(wnds);
        listener.mouse_leave(wm, &hwnd);

        Some(())
    })();
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_discrete_scroll_handler(
    wnd_ptr: usize,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) {
    if let Some((wm, hwnd, listener)) = with_wnd_mut(
        unsafe { Wm::global_unchecked() },
        wnd_ptr,
        |wnd, hwnd, wm| (wm, hwnd, Rc::clone(&wnd.listener)),
    ) {
        listener.scroll_motion(
            wm,
            &hwnd,
            [x, y].into(),
            &iface::ScrollDelta {
                delta: [delta_x, delta_y].into(),
                precise: false,
            },
        );
    }
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_smooth_scroll_handler(
    wnd_ptr: usize,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) {
    log::warn!(
        "TODO: tcw_wnd_widget_smooth_scroll_handler{:?}",
        (wnd_ptr, x, y, delta_x, delta_y)
    );
}

#[no_mangle]
extern "C" fn tcw_wnd_widget_smooth_scroll_stop_handler(wnd_ptr: usize) {
    log::warn!("TODO: tcw_wnd_widget_smooth_scroll_stop_handler");
}

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
    wnd_ptr: Cell<usize>,
}

#[repr(C)]
pub struct TcwWndWidgetClass {
    parent_class: gtk_sys::GtkDrawingAreaClass,
}

impl WndWidget {
    fn new(_: Wm) -> Self {
        // We have `Wm`, so we know we are on the main thread, hence this is safe
        unsafe { gtk::Widget::from_glib_none(tcw_wnd_widget_new()).unsafe_cast() }
    }

    fn wnd_ptr(&self) -> &Cell<usize> {
        unsafe { &(&*self.as_ptr()).wnd_ptr }
    }
}
