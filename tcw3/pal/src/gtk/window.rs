use glib::{
    glib_object_wrapper, glib_wrapper,
    translate::{FromGlibPtrBorrow, FromGlibPtrFull, FromGlibPtrNone, ToGlibPtr},
};
use gtk::prelude::*;
use iterpool::{Pool, PoolPtr};
use log::warn;
use std::{
    cell::{Cell, RefCell},
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

struct Wnd {
    gtk_wnd: gtk::Window,
    gtk_widget: WndWidget,
    comp_wnd: comp::Wnd,
    // TODO: Handle the following events:
    //       - update_ready
    //       - resize
    //       - mouse_motion
    //       - mouse_leave
    //       - mouse_drag
    //       - scroll_motion
    //       - scroll_gesture
    listener: Rc<dyn iface::WndListener<Wm>>,
}

impl HWnd {
    /// Implements `Wm::new_wnd`.
    pub(super) fn new_wnd(wm: Wm, mut attrs: WndAttrs<'_>) -> Self {
        let gtk_wnd = gtk::Window::new(gtk::WindowType::Toplevel);

        let gtk_widget = WndWidget::new(wm);

        gtk_wnd.add(&gtk_widget);
        gtk_wnd.set_hexpand(true);
        gtk_wnd.set_vexpand(true);

        let comp_wnd = COMPOSITOR
            .get_with_wm(wm)
            .borrow_mut()
            .new_wnd(attrs.layer.take().unwrap_or(None));

        let wnd = Wnd {
            gtk_wnd,
            gtk_widget,
            comp_wnd,
            listener: Rc::new(()),
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

        let default_geom = gdk::Geometry {
            min_width: attrs.min_size.unwrap_or_default()[0] as i32,
            min_height: attrs.min_size.unwrap_or_default()[0] as i32,
            max_width: attrs.max_size.unwrap_or_default()[0] as i32,
            max_height: attrs.max_size.unwrap_or_default()[0] as i32,
            base_width: 0,
            base_height: 0,
            width_inc: 0,
            height_inc: 0,
            min_aspect: 0.0,
            max_aspect: 0.0,
            win_gravity: gdk::Gravity::NorthWest,
        };
        let mut hint_flags = gdk::WindowHints::empty();
        hint_flags.set(gdk::WindowHints::MIN_SIZE, attrs.min_size.is_some());
        hint_flags.set(gdk::WindowHints::MAX_SIZE, attrs.max_size.is_some());

        if !hint_flags.is_empty() {
            wnd.gtk_wnd
                .set_geometry_hints(None::<&gtk::Widget>, Some(&default_geom), hint_flags);
        }

        if let Some(size) = attrs.size {
            wnd.gtk_wnd.resize(size[0] as i32, size[1] as i32);
            wnd.gtk_wnd.set_default_size(size[0] as i32, size[1] as i32);
        }

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

        COMPOSITOR
            .get_with_wm(wm)
            .borrow_mut()
            .remove_wnd(&wnd.comp_wnd);
    }

    /// Implements `Wm::update_wnd`.
    pub(super) fn update_wnd(&self, wm: Wm) {
        let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
        let wnd = &mut wnds[self.ptr];

        if !wnd.gtk_wnd.is_visible() {
            return;
        }

        let (surf_size, dpi_scale) = comp_surf_props_for_gtk_wnd(&wnd.gtk_wnd);

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
            wnd.gtk_wnd.queue_draw_area(x, y, width, height);
        }
    }

    /// Implements `Wm::get_wnd_size`.
    pub(super) fn get_wnd_size(&self, wm: Wm) -> [u32; 2] {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let gtk_wnd = &wnds[self.ptr].gtk_wnd;
        [
            gtk_wnd.get_allocated_width() as u32,
            gtk_wnd.get_allocated_height() as u32,
        ]
    }

    /// Implements `Wm::get_wnd_dpi_scale`.
    pub(super) fn get_wnd_dpi_scale(&self, wm: Wm) -> f32 {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let gtk_wnd = &wnds[self.ptr].gtk_wnd;
        gtk_wnd.get_scale_factor() as f32
    }

    /// Implements `Wm::request_update_ready_wnd`.
    pub(super) fn request_update_ready_wnd(&self, _wm: Wm) {
        // TODO
    }
}

fn comp_surf_props_for_gtk_wnd(gtk_wnd: &gtk::Window) -> ([usize; 2], f32) {
    let factor = gtk_wnd.get_scale_factor() as usize;

    (
        [
            gtk_wnd.get_allocated_width() as usize * factor,
            gtk_wnd.get_allocated_height() as usize * factor,
        ],
        factor as f32,
    )
}

/// Used by `TcwWndWidget`'s callback functions. Mutably borrow `WNDS` and
/// call the given closure with `Wnd`, `HWnd`, and `Wm`.
fn with_wnd_mut<R>(wm: Wm, wnd_ptr: usize, f: impl FnOnce(&mut Wnd, HWnd, Wm) -> R) -> Option<R> {
    use std::num::NonZeroUsize;
    let ptr = PoolPtr(NonZeroUsize::new(wnd_ptr).unwrap());

    let mut wnds = WNDS.get_with_wm(wm).borrow_mut();
    if let Some(wnd) = wnds.get_mut(ptr) {
        Some(f(wnd, HWnd { ptr }, wm))
    } else {
        warn!("Ignoring invalid window ptr: {:?}", ptr);
        None
    }
}

/// Handles `GtkWidgetClass::draw`. `wnd_ptr` is retrieved from
/// `TcwWndWidget::wnd_ptr`.
#[no_mangle]
extern "C" fn tcw_wnd_widget_draw_handler(wnd_ptr: usize, cairo_ctx: *mut cairo_sys::cairo_t) {
    with_wnd_mut(unsafe { Wm::global_unchecked() }, wnd_ptr, |wnd, _, wm| {
        let mut compositor = COMPOSITOR.get_with_wm(wm).borrow_mut();

        let (surf_size, dpi_scale) = comp_surf_props_for_gtk_wnd(&wnd.gtk_wnd);
        compositor.update_wnd(&mut wnd.comp_wnd, surf_size, dpi_scale, false);

        compositor.paint_wnd(&mut wnd.comp_wnd);

        let cr = unsafe { cairo::Context::from_glib_borrow(cairo_ctx) };
        if let Some(surface) = wnd.comp_wnd.cairo_surface() {
            cr.set_source_surface(surface, 0.0, 0.0);
            cr.set_operator(cairo::Operator::Over);
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
