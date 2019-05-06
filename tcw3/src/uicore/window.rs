use bitflags::bitflags;
use cggeom::{prelude::*, Box2};
use cgmath::Point2;
use flags_macro::flags;
use std::{
    cell::RefCell,
    cmp::{max, min},
    rc::{Rc, Weak},
};

use super::{
    HView, HWnd, Superview, SuperviewStrong, UpdateCtx, ViewDirtyFlags, ViewFlags, ViewListener,
    Wnd, WndStyleFlags,
};
use crate::pal::{self, prelude::WM as _, WM};

impl HView {
    /// Get the containing window for a view.
    pub(super) fn containing_wnd(&self) -> Option<HWnd> {
        match self.view.superview.borrow().upgrade() {
            None => None,
            Some(SuperviewStrong::View(sv)) => HView { view: sv }.containing_wnd(),
            Some(SuperviewStrong::Window(wnd)) => Some(HWnd { wnd }),
        }
    }
}

impl HWnd {
    fn ensure_materialized(&self) {
        assert!(!self.wnd.closed.get(), "the window has been already closed");

        let mut pal_wnd_cell = self.wnd.pal_wnd.borrow_mut();
        if pal_wnd_cell.is_some() {
            return;
        }

        let mut attrs = pal::WndAttrs {
            listener: Some(Some(Rc::new(PalWndListener {
                wnd: Rc::downgrade(&self.wnd),
            }))),
            ..Default::default()
        };
        let dirty = &self.wnd.dirty;
        let style_attrs = self.wnd.style_attrs.borrow();

        style_attrs.transfer_to_pal(dirty.get(), &mut attrs);
        dirty.set(dirty.get() - WndDirtyFlags::style());

        let pal_wnd = self.wnd.wm.new_wnd(&attrs);
        *pal_wnd_cell = Some(pal_wnd);
    }

    /// Pend an update.
    ///
    /// This is layers are layouted and rendered. Also, the update process
    /// clears `Wnd::dirty`.
    pub(super) fn pend_update(&self) {
        assert!(!self.wnd.closed.get(), "the window has been already closed");

        // Already queued?
        let dirty = &self.wnd.dirty;
        if dirty.get().contains(WndDirtyFlags::UPDATE) {
            return;
        }
        dirty.set(dirty.get() | WndDirtyFlags::UPDATE);

        let hwnd: HWnd = self.clone();

        self.wnd.wm.invoke(move |_| {
            hwnd.update();
        });
    }

    fn update(&self) {
        // Clear the flag
        {
            let dirty = &self.wnd.dirty;
            dirty.set(dirty.get() - WndDirtyFlags::UPDATE);
        }

        if self.wnd.closed.get() {
            return;
        }

        // Clear views' dirty flags
        if self.wnd.style_attrs.borrow().visible {
            self.ensure_materialized();
        }

        let pal_wnd = self.wnd.pal_wnd.borrow();
        let pal_wnd = if let Some(ref pal_wnd) = *pal_wnd {
            pal_wnd
        } else {
            return;
        };

        let (new_size, min_size, max_size) = self.update_views();

        // Update the window's attributes
        let mut attrs = pal::WndAttrs::default();
        let dirty = self.wnd.dirty.replace(WndDirtyFlags::empty());

        if dirty.contains(WndDirtyFlags::LAYER) {
            let view = self.wnd.content_view.borrow();
            let layers = view.as_ref().unwrap().view.layers.borrow();

            debug_assert_eq!(
                layers.len(),
                1,
                "the root view must provide exactly one layer"
            );
            attrs.layer = Some(Some(layers[0].clone()));
        }

        if dirty.contains(WndDirtyFlags::DEFAULT_SIZE) {
            // This flag is handled by `update_views`
            debug_assert!(new_size.is_some());
        }

        attrs.min_size = min_size;
        attrs.max_size = max_size;
        attrs.size = new_size;

        // Apply changes in `WndStyleAttrs`
        let style_attrs = self.wnd.style_attrs.borrow();
        style_attrs.transfer_to_pal(dirty, &mut attrs);

        // Suppress resize events (caused by `set_wnd_attr`)
        self.wnd.updating.set(true);

        // Update PAL window attributes
        self.wnd.wm.set_wnd_attr(pal_wnd, &attrs);

        // Un-suppress resize events
        self.wnd.updating.set(false);

        // Update layers
        self.wnd.wm.update_wnd(pal_wnd);
    }

    /// Perform pending updates. Also, returns a new, min, and max window size
    /// based on the `SizeTraits` of the root view.
    fn update_views(&self) -> (Option<[u32; 2]>, Option<[u32; 2]>, Option<[u32; 2]>) {
        let pal_wnd = self.wnd.pal_wnd.borrow();
        let pal_wnd = pal_wnd.as_ref().unwrap();

        let mut new_size = None;
        let mut min_size = None;
        let mut max_size = None;

        let resize_to_preferred = self.wnd.dirty.get().contains(WndDirtyFlags::DEFAULT_SIZE);

        // Repeat until the update converges...
        for _ in 0..100 {
            let view: HView = self.wnd.content_view.borrow().clone().unwrap();

            if !view.view.dirty.get().is_dirty() {
                return (new_size, min_size, max_size);
            }

            view.call_pending_mount_if_dirty(self.wnd.wm, self);

            // Layout: down phase
            view.update_size_traits();

            // Constrain the window size
            let size_traits = view.view.size_traits.get();
            let mut wnd_size = self.wnd.wm.get_wnd_size(pal_wnd);

            if resize_to_preferred {
                wnd_size = [
                    size_traits.preferred.x as u32,
                    size_traits.preferred.y as u32,
                ];
            }

            let min_s = [
                size_traits.min.x.ceil() as u32,
                size_traits.min.y.ceil() as u32,
            ];
            let max_s = [
                size_traits.max.x.min(<u32>::max_value() as f32) as u32,
                size_traits.max.y.min(<u32>::max_value() as f32) as u32,
            ];

            let max_s = [max(max_s[0], min_s[0]), max(max_s[1], min_s[1])];

            let new_wnd_size = [
                min(max(wnd_size[0], min_s[0]), max_s[0]),
                min(max(wnd_size[1], min_s[1]), max_s[1]),
            ];
            if new_wnd_size != wnd_size || resize_to_preferred {
                // Resize the window to satisfy the constraint
                new_size = Some(new_wnd_size);
            }

            min_size = Some(min_s);
            max_size = Some(max_s);

            // Resize the root view to fit the window
            let new_frame = Box2::new(
                Point2::new(0.0, 0.0),
                Point2::new(new_wnd_size[0], new_wnd_size[1])
                    .cast()
                    .unwrap(),
            );
            if new_frame != view.view.frame.get() {
                view.view.frame.set(new_frame);
                view.set_dirty_flags(ViewDirtyFlags::SUBVIEWS_FRAME);
            }

            // Layout: up phase
            view.update_subview_frames();

            // Position views
            view.flush_position_event(self.wnd.wm);

            // Update visual
            view.update_layers(self.wnd.wm, self);
        }

        panic!("Window update did not converge");
    }
}

impl Wnd {
    pub(super) fn close(&self) {
        if self.closed.get() {
            return;
        }

        // Detach the content view
        {
            let view: HView = self.content_view.borrow_mut().take().unwrap();

            debug_assert!(std::ptr::eq(
                self,
                // Get the superview
                &*view
                    .view
                    .superview
                    .borrow()
                    // Assuming it's a window...
                    .wnd()
                    .unwrap()
                    // It should be still valid...
                    .upgrade()
                    .unwrap()
            ));

            *view.view.superview.borrow_mut() = Superview::empty();

            view.call_unmount(self.wm);
        }

        if let Some(hwnd) = self.pal_wnd.borrow_mut().take() {
            // TODO: should clarify whether `pal::WndListener::close` is called or not
            self.wm.remove_wnd(&hwnd);
        }

        self.closed.set(true);
    }

    /// Set dirty flags on a window.
    pub(super) fn set_dirty_flags(&self, new_flags: WndDirtyFlags) {
        let dirty = &self.dirty;
        dirty.set(dirty.get() | new_flags);
    }
}

impl Drop for Wnd {
    fn drop(&mut self) {
        self.close();
    }
}

struct PalWndListener {
    wnd: Weak<Wnd>,
}

impl PalWndListener {
    /// Get `HWnd` if the underlying object is still alive.
    fn hwnd(&self) -> Option<HWnd> {
        self.wnd.upgrade().map(|wnd| HWnd { wnd })
    }
}

impl pal::iface::WndListener<WM> for PalWndListener {
    fn close_requested(&self, wm: WM, _: &pal::HWnd) -> bool {
        if let Some(hwnd) = self.hwnd() {
            let listener = hwnd.wnd.listener.borrow();
            listener.close_requested(wm, &hwnd)
        } else {
            true
        }
    }

    fn close(&self, wm: WM, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            hwnd.close();

            let listener = hwnd.wnd.listener.borrow();
            listener.close(wm, &hwnd);
        }
    }

    fn resize(&self, _: WM, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            if hwnd.wnd.updating.get() {
                // Prevent recursion
                return;
            }

            {
                let view = hwnd.wnd.content_view.borrow();
                let view = view.as_ref().unwrap();

                view.set_dirty_flags(ViewDirtyFlags::SUBVIEWS_FRAME);
            }

            // Layers should be updated *within* the call to thie method
            // for them to properly follow the window outline being dragged.
            hwnd.update();
        }
    }

    fn dpi_scale_changed(&self, _: WM, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            let handlers = hwnd.wnd.dpi_scale_changed_handlers.borrow();
            for handler in handlers.iter() {
                handler(hwnd.wnd.wm, &hwnd);
            }
        }
    }
}

pub(crate) fn new_root_content_view() -> HView {
    let view = HView::new(ViewFlags::LAYER_GROUP);
    view.set_listener(Box::new(RootViewListener::new()));
    view
}

struct RootViewListener {
    layer: RefCell<Option<pal::HLayer>>,
}

impl RootViewListener {
    fn new() -> Self {
        Self {
            layer: RefCell::new(None),
        }
    }
}

impl ViewListener for RootViewListener {
    fn mount(&self, wm: WM, _: &HView, _: &HWnd) {
        *self.layer.borrow_mut() = Some(wm.new_layer(&pal::LayerAttrs {
            // `bounds` mustn't be empty, so...
            bounds: Some(Box2::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0))),
            ..Default::default()
        }));
    }

    fn unmount(&self, wm: WM, _: &HView) {
        if let Some(hlayer) = self.layer.borrow_mut().take() {
            wm.remove_layer(&hlayer);
        }
    }

    fn update(&self, wm: WM, _: &HView, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.borrow();
        let layer = layer.as_ref().unwrap();

        if let Some(sublayers) = ctx.sublayers().take() {
            wm.set_layer_attr(
                &layer,
                &pal::LayerAttrs {
                    sublayers: Some(sublayers),
                    ..Default::default()
                },
            );
        }

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![(*layer).clone()]);
        }
    }
}

bitflags! {
    /// Indicates which properties should be updated when `Wnd::update` is
    /// called for the next time.
    ///
    /// Be aware that the usage is different from that of `ViewDirtyFlags`.
    pub struct WndDirtyFlags: u8 {
        /// The root layer should be updated.
        const LAYER = 1 << 0;
        /// The window should be resized to the default size.
        const DEFAULT_SIZE = 1 << 1;

        const STYLE_VISIBLE = 1 << 2;
        const STYLE_FLAGS = 1 << 3;
        const STYLE_CAPTION = 1 << 4;

        /// `update` is queued to the main event queue.
        const UPDATE = 1 << 5;
    }
}

impl Default for WndDirtyFlags {
    fn default() -> Self {
        WndDirtyFlags::all() - WndDirtyFlags::UPDATE
    }
}

impl WndDirtyFlags {
    fn style() -> Self {
        flags![WndDirtyFlags::{STYLE_VISIBLE | STYLE_FLAGS | STYLE_CAPTION}]
    }
}

#[derive(Debug)]
pub(super) struct WndStyleAttrs {
    pub flags: WndStyleFlags,
    pub caption: String,
    pub visible: bool,
}

impl Default for WndStyleAttrs {
    fn default() -> Self {
        Self {
            flags: WndStyleFlags::default(),
            caption: "TCW3 Window".to_owned(),
            visible: false,
        }
    }
}

impl WndStyleAttrs {
    fn transfer_to_pal<'a>(&'a self, dirty: WndDirtyFlags, attrs: &mut pal::WndAttrs<&'a str>) {
        if dirty.contains(WndDirtyFlags::STYLE_VISIBLE) {
            attrs.visible = Some(self.visible);
        }
        if dirty.contains(WndDirtyFlags::STYLE_FLAGS) {
            attrs.flags = Some(self.flags);
        }
        if dirty.contains(WndDirtyFlags::STYLE_CAPTION) {
            attrs.caption = Some(self.caption.as_ref());
        }
    }
}
