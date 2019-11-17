use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use winit::{
    event::{ElementState, WindowEvent},
    window::{Window, WindowBuilder, WindowId},
};

use super::super::iface::{WndAttrs, WndFlags};
use super::{
    utils::{log_pos_to_point2, mouse_button_to_id},
    HWndCore, WinitWm, WinitWmCore, Wnd, WndContent, WndMouseDrag,
};

impl<TWM: WinitWm, TWC: WndContent<Wm = TWM>> WinitWmCore<TWM, TWC> {
    /// Create a window and return a window handle.
    ///
    /// `content_factory` is a function that constructs a backend-specific
    /// structure for rendering window contents.
    pub fn new_wnd(
        &self,
        attrs: WndAttrs<'_, TWM, TWC::HLayer>,
        content_factory: impl FnOnce(&Window, Option<TWC::HLayer>) -> TWC,
    ) -> HWndCore {
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
            mouse_drag: RefCell::new(None),
            mouse_pos: Cell::new((0.0, 0.0).into()),
            waiting_update_ready: Cell::new(false),
        };

        let ptr = self.wnds.borrow_mut().allocate(Rc::new(wnd));

        HWndCore { ptr }
    }

    pub fn set_wnd_attr(&self, hwnd: &HWndCore, attrs: WndAttrs<'_, TWM, TWC::HLayer>) {
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
        if let Some(x) = attrs.listener {
            *wnd.listener.borrow_mut() = x;
        }
    }

    pub fn remove_wnd(&self, hwnd: &HWndCore) {
        let wnd = self.wnds.borrow_mut().deallocate(hwnd.ptr).unwrap();

        // And then call `WndContent::close`
        wnd.content.borrow_mut().close(self, &wnd.winit_wnd);
    }

    pub fn update_wnd(&self, hwnd: &HWndCore) {
        let wnd = &self.wnds.borrow()[hwnd.ptr];
        let wants_request_redraw = wnd.content.borrow_mut().update(self, &wnd.winit_wnd);

        if wants_request_redraw && !self.suppress_request_redraw.get() {
            wnd.winit_wnd.request_redraw();
        }
    }

    pub fn request_update_ready_wnd(&self, hwnd: &HWndCore) {
        let wnd = &self.wnds.borrow()[hwnd.ptr];
        wnd.waiting_update_ready.set(true);

        wnd.winit_wnd.request_redraw();
    }

    pub fn get_wnd_size(&self, hwnd: &HWndCore) -> [u32; 2] {
        let wnd = &self.wnds.borrow()[hwnd.ptr];

        let size = wnd.winit_wnd.inner_size();

        // Truncating fractions. The ramification is unknonw.
        [size.width as u32, size.height as u32]
    }
    pub fn get_wnd_dpi_scale(&self, hwnd: &HWndCore) -> f32 {
        let wnd = &self.wnds.borrow()[hwnd.ptr];

        wnd.winit_wnd.hidpi_factor() as f32
    }

    pub(super) fn handle_wnd_evt(&self, wnd_id: WindowId, evt: WindowEvent) {
        let (wnd, hwnd_core);

        if let Some((ptr, wnd_ref)) = self
            .wnds
            .borrow()
            .ptr_iter()
            .find(|(_, w)| w.winit_wnd.id() == wnd_id)
        {
            wnd = Rc::clone(wnd_ref);
            hwnd_core = HWndCore { ptr }
        } else {
            return;
        }

        let listener = wnd.listener.borrow();

        let hwnd = self.wm().hwnd_core_to_hwnd(&hwnd_core);

        match evt {
            WindowEvent::Resized(_) => {
                let guard = SuppressRequestRedrawGuard::new(self);
                listener.resize(self.wm(), &hwnd);

                if wnd.waiting_update_ready.take() {
                    listener.update_ready(self.wm(), &hwnd);
                }
                drop(guard);

                // I thought `Resized` implies `RedrawRequested` is automatically
                // called. Without this, the window content and the size gets
                // desynced on macOS + `unix` backend.
                drop(listener);
                wnd.content
                    .borrow_mut()
                    .redraw_requested(self, &wnd.winit_wnd);
            }
            WindowEvent::CloseRequested => {
                listener.close_requested(self.wm(), &hwnd);
            }
            WindowEvent::CursorMoved { position, .. } => {
                wnd.mouse_pos.set(log_pos_to_point2(position));

                if let Some(ref mut mouse_drag) = &mut *wnd.mouse_drag.borrow_mut() {
                    mouse_drag
                        .listener
                        .mouse_motion(self.wm(), &hwnd, log_pos_to_point2(position));
                } else {
                    listener.mouse_motion(self.wm(), &hwnd, log_pos_to_point2(position));
                }
            }
            WindowEvent::CursorLeft { .. } => {
                listener.mouse_leave(self.wm(), &hwnd);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mut mouse_drag_cell = wnd.mouse_drag.borrow_mut();

                // `MouseInput` doesn't provide the coordinates, so we get them
                // from the last `CursorMoved` event
                let mouse_pos = wnd.mouse_pos.get();

                let mouse_button = mouse_button_to_id(button);
                if mouse_button >= 64 {
                    // Can't fit within our bitfield's range, ignore the event
                    return;
                }
                let mouse_button_mask = 1u64 << mouse_button;

                match state {
                    ElementState::Pressed => {
                        let mouse_drag = mouse_drag_cell.get_or_insert_with(|| {
                            // `wnd.mouse_drag` is `None`. Initiate a mouse drag
                            // gesture
                            let drag_listener =
                                listener.mouse_drag(self.wm(), &hwnd, mouse_pos, mouse_button);

                            WndMouseDrag {
                                listener: drag_listener,
                                pressed_buttons: 0,
                            }
                        });

                        if (mouse_drag.pressed_buttons & mouse_button_mask) != 0 {
                            // Already pressed?
                            return;
                        }

                        mouse_drag.pressed_buttons |= mouse_button_mask;

                        mouse_drag
                            .listener
                            .mouse_down(self.wm(), &hwnd, mouse_pos, mouse_button);
                    }
                    ElementState::Released => {
                        let mouse_drag = if let Some(x) = &mut *mouse_drag_cell {
                            x
                        } else {
                            // No mouse drag gesture that we know is active, ignoring
                            // the event
                            return;
                        };

                        if (mouse_drag.pressed_buttons & mouse_button_mask) == 0 {
                            // We think the button hasn't been pressed
                            return;
                        }

                        mouse_drag.pressed_buttons &= !mouse_button_mask;

                        mouse_drag
                            .listener
                            .mouse_up(self.wm(), &hwnd, mouse_pos, mouse_button);

                        // End the mouse drag gesture if no buttons are pressed anymore
                        if mouse_drag.pressed_buttons == 0 {
                            *mouse_drag_cell = None;
                        }
                    }
                } // match state
            } // WindowEvent::MouseInput
            WindowEvent::RedrawRequested => {
                if wnd.waiting_update_ready.take() {
                    let _guard = SuppressRequestRedrawGuard::new(self);
                    listener.update_ready(self.wm(), &hwnd);
                }
                drop(listener);

                wnd.content
                    .borrow_mut()
                    .redraw_requested(self, &wnd.winit_wnd);
            }
            WindowEvent::HiDpiFactorChanged(_) => {
                listener.dpi_scale_changed(self.wm(), &hwnd);
            }
            _ => {}
        }
    }
}

/// An RAII guard to temporarily set `WinitWmCore::suppress_request_redraw`.
///
/// `WndListener::update_ready` may call `update_wnd`. Setting
/// `suppress_request_redraw` prevents `update_wnd` from calling
/// `request_redraw`.
struct SuppressRequestRedrawGuard<'a>(&'a Cell<bool>);

impl<'a> SuppressRequestRedrawGuard<'a> {
    fn new(x: &'a WinitWmCore<impl WinitWm, impl WndContent>) -> Self {
        let cell = &x.suppress_request_redraw;
        debug_assert!(!cell.get());
        cell.set(true);
        Self(cell)
    }
}

impl Drop for SuppressRequestRedrawGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
