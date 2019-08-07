use std::{cell::RefCell, rc::Rc};
use winit::window::{Window, WindowBuilder};

use super::super::iface::{WndAttrs, WndFlags};
use super::{HWnd, WinitWm, WinitWmWrap, Wnd, WndContent};

impl<TWM: WinitWmWrap, TWC: WndContent<Wm = TWM>> WinitWm<TWM, TWC> {
    /// Create a window and return a window handle.
    ///
    /// `content_factory` is a function that constructs a backend-specific
    /// structure for rendering window contents.
    pub fn new_wnd(
        &self,
        attrs: WndAttrs<'_, TWM, TWC::HLayer>,
        content_factory: impl FnOnce(&Window, Option<TWC::HLayer>) -> TWC,
    ) -> HWnd {
        let winit_wnd = self.with_event_loop_wnd_target(|el_wnd_target| {
            let mut builder = WindowBuilder::new();

            if let Some([w, h]) = attrs.size {
                builder = builder.with_inner_size((w, h).into());
            }
            if let Some([w, h]) = attrs.min_size {
                builder = builder.with_min_inner_size((w, h).into());
            }
            if let Some([w, h]) = attrs.max_size {
                builder = builder.with_max_inner_size((w, h).into());
            }
            if let Some(flags) = attrs.flags {
                builder = builder.with_decorations(!flags.contains(WndFlags::BORDERLESS));
                builder = builder.with_resizable(flags.contains(WndFlags::RESIZABLE));
            }
            if let Some(title) = &attrs.caption {
                builder = builder.with_title(&**title);
            }
            if let Some(x) = attrs.visible {
                builder = builder.with_visible(x);
            }

            builder
                .build(el_wnd_target)
                .expect("could not create a window")
        });

        let layer = attrs.layer.unwrap_or_default();
        let listener = attrs.listener.unwrap_or_else(|| Box::new(()));

        let content = content_factory(&winit_wnd, layer);

        let wnd = Wnd {
            winit_wnd,
            content: RefCell::new(content),
            listener: RefCell::new(listener),
        };

        let ptr = self.wnds.borrow_mut().allocate(Rc::new(wnd));

        HWnd { ptr }
    }

    pub fn set_wnd_attr(&self, hwnd: &HWnd, attrs: WndAttrs<'_, TWM, TWC::HLayer>) {
        let wnd = &self.wnds.borrow()[hwnd.ptr];
        let winit_wnd = &wnd.winit_wnd;

        if let Some([w, h]) = attrs.size {
            winit_wnd.set_inner_size((w, h).into());
        }
        if let Some([w, h]) = attrs.min_size {
            winit_wnd.set_min_inner_size(Some((w, h).into()));
        }
        if let Some([w, h]) = attrs.max_size {
            winit_wnd.set_max_inner_size(Some((w, h).into()));
        }
        if let Some(flags) = attrs.flags {
            winit_wnd.set_decorations(!flags.contains(WndFlags::BORDERLESS));
            winit_wnd.set_resizable(flags.contains(WndFlags::RESIZABLE));
        }
        if let Some(title) = &attrs.caption {
            winit_wnd.set_title(&**title);
        }
        if let Some(x) = attrs.visible {
            winit_wnd.set_visible(x);
        }
        if let Some(x) = attrs.layer {
            wnd.content.borrow_mut().set_layer(self, &wnd.winit_wnd, x);
        }
    }

    pub fn remove_wnd(&self, hwnd: &HWnd) {
        // Call `WndListener::close`. Note that we must unborrow `RefCell`
        // before calling into `WndListener`.
        let wnd = self.wnds.borrow_mut().deallocate(hwnd.ptr).unwrap();
        let outer_hwnd = self.wm().winit_hwnd_to_hwnd(hwnd);
        wnd.listener.borrow().close(self.wm(), &outer_hwnd);

        // And then call `WndContent::close`
        wnd.content.borrow_mut().close(self, &wnd.winit_wnd);
    }

    pub fn update_wnd(&self, hwnd: &HWnd) {
        let wnd = &self.wnds.borrow()[hwnd.ptr];
        wnd.content.borrow_mut().update(self, &wnd.winit_wnd);
    }

    pub fn get_wnd_size(&self, hwnd: &HWnd) -> [u32; 2] {
        let wnd = &self.wnds.borrow()[hwnd.ptr];

        let size = wnd.winit_wnd.inner_size();

        // Truncating fractions. The ramification is unknonw.
        [size.width as u32, size.height as u32]
    }
    pub fn get_wnd_dpi_scale(&self, hwnd: &HWnd) -> f32 {
        let wnd = &self.wnds.borrow()[hwnd.ptr];

        wnd.winit_wnd.hidpi_factor() as f32
    }
}
