use glib::{
    glib_object_wrapper, glib_wrapper,
    translate::{FromGlibPtrFull, FromGlibPtrNone, ToGlibPtr},
};
use gtk::prelude::*;
use iterpool::{Pool, PoolPtr};
use std::cell::RefCell;

use super::{Wm, WndAttrs};
use crate::{iface, MtSticky};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd {
    ptr: PoolPtr,
}

static WNDS: MtSticky<RefCell<Pool<Wnd>>, Wm> = {
    // `Wnd` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(Pool::new())) }
};

struct Wnd {
    gtk_wnd: gtk::Window,
    gtk_widget: WndWidget,
}

impl HWnd {
    /// Implements `Wm::new_wnd`.
    pub(super) fn new_wnd(wm: Wm, attrs: WndAttrs<'_>) -> Self {
        let gtk_wnd = gtk::Window::new(gtk::WindowType::Toplevel);

        let gtk_widget = WndWidget::new(wm);

        gtk_wnd.add(&gtk_widget);
        gtk_wnd.set_hexpand(true);
        gtk_wnd.set_vexpand(true);

        let wnd = Wnd {
            gtk_wnd,
            gtk_widget,
        };
        let ptr = WNDS.get_with_wm(wm).borrow_mut().allocate(wnd);
        let this = Self { ptr };
        this.set_wnd_attr(wm, attrs);
        this
    }

    /// Implements `Wm::set_wnd_attr`.
    pub(super) fn set_wnd_attr(&self, wm: Wm, attrs: WndAttrs<'_>) {
        let wnds = WNDS.get_with_wm(wm).borrow();
        let wnd = &wnds[self.ptr];

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

        // TODO: listener
        // TODO: layer
        // TODO: cursor_shape

        if let Some(caption) = attrs.caption {
            wnd.gtk_wnd.set_title(&caption);
        }

        if let Some(visible) = attrs.visible {
            if visible {
                wnd.gtk_wnd.show();
            } else {
                wnd.gtk_wnd.hide();
            }
        }

        // TODO
    }

    /// Implements `Wm::remove_wnd`.
    pub(super) fn remove_wnd(&self, wm: Wm) {
        WNDS.get_with_wm(wm).borrow_mut().deallocate(self.ptr);
    }

    /// Implements `Wm::update_wnd`.
    pub(super) fn update_wnd(&self, wm: Wm) {
        // TODO
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
    pub(super) fn request_update_ready_wnd(&self, wm: Wm) {
        // TODO
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
}

#[repr(C)]
pub struct TcwWndWidgetClass {
    parent_class: gtk_sys::GtkDrawingAreaClass,
}

impl WndWidget {
    fn new(wm: Wm) -> Self {
        unsafe { gtk::Widget::from_glib_none(tcw_wnd_widget_new()).unsafe_cast() }
    }
}
