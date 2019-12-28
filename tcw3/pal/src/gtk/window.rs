use array::Array2;
use gtk::prelude::*;
use iterpool::{Pool, PoolPtr};
use std::cell::RefCell;

use super::{Wm, WndAttrs};
use crate::{iface, MtSticky};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HWnd {
    ptr: PoolPtr,
}

static WNDS: MtSticky<RefCell<Pool<Wnd>>> = {
    // `Wnd` is `!Send`, but there is no instance at this point, so this is safe
    unsafe { MtSticky::new_unchecked(RefCell::new(Pool::new())) }
};

struct Wnd {
    gtk_wnd: gtk::Window,
    gtk_widget: gtk::DrawingArea,
}

impl HWnd {
    /// Implements `Wm::new_wnd`.
    pub(super) fn new_wnd(wm: Wm, attrs: WndAttrs<'_>) -> Self {
        let gtk_wnd = gtk::Window::new(gtk::WindowType::Toplevel);

        let gtk_widget = gtk::DrawingArea::new();

        gtk_wnd.add(&gtk_widget);


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

        // TOOD: size
        // TODO: min_size
        // TODO: max_size

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
            gtk_wnd.get_allocated_height() as u32 ,
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
