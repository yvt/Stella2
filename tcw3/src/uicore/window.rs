use alt_fp::FloatOrd;
use bitflags::bitflags;
use cggeom::box2;
use cgmath::Point2;
use flags_macro::flags;
use neo_linked_list::{linked_list::Node, AssertUnpin};
use rc_borrow::RcBorrow;
use std::{
    cell::RefCell,
    cmp::{max, min},
    pin::Pin,
    rc::Weak,
};

use super::{
    invocation::process_pending_invocations, CursorShape, HView, HViewRef, HWnd, HWndRef,
    Superview, SuperviewStrong, UpdateCtx, ViewDirtyFlags, ViewFlags, ViewListener, Wnd,
    WndStyleFlags,
};
use crate::pal::{self, prelude::*, Wm};

impl HViewRef<'_> {
    /// Get the containing window for a view.
    pub fn containing_wnd(self) -> Option<HWnd> {
        match self.view.superview.borrow().upgrade() {
            None => None,
            Some(SuperviewStrong::View(sv)) => HView { view: sv }.as_ref().containing_wnd(),
            Some(SuperviewStrong::Window(wnd)) => Some(HWnd { wnd }),
        }
    }
}

/// A new, min, and max window size based on the `SizeTraits` of the root view.
#[derive(Default)]
struct RootSizeReq {
    new_size: Option<[u32; 2]>,
    min_size: Option<[u32; 2]>,
    max_size: Option<[u32; 2]>,
}

impl HWndRef<'_> {
    /// Get the PAL window handle of the window.
    ///
    /// Returns `None` if the window hasn't been materialized yet or the window
    /// has already been closed. Materialization takes place before the window
    /// is updated and the subviews are mounted for the first time.
    pub fn pal_hwnd(self) -> Option<pal::HWnd> {
        self.wnd.pal_wnd.borrow().clone()
    }

    fn ensure_materialized(self) {
        assert!(!self.wnd.closed.get(), "the window has been already closed");

        let mut pal_wnd_cell = self.wnd.pal_wnd.borrow_mut();
        if pal_wnd_cell.is_some() {
            return;
        }

        let style_attrs = self.wnd.style_attrs.borrow();
        let dirty = &self.wnd.dirty;
        let mut transferred_flags = WndDirtyFlags::style();
        if style_attrs.visible {
            // Don't make the window visible just yet - the size is
            // not calculated yet and the contents are empty
            transferred_flags -= WndDirtyFlags::STYLE_VISIBLE;
        }
        dirty.set(dirty.get() - transferred_flags);

        let mut attrs = Default::default();
        style_attrs.transfer_to_pal(transferred_flags, &mut attrs);

        let pal_wnd = self.wnd.wm.new_wnd(attrs);
        *pal_wnd_cell = Some(pal_wnd);

        drop(pal_wnd_cell);
        drop(style_attrs);

        // Set the listener after creating the window. The listener's methods
        // expect `pal_wnd` to be borrowable.
        let pal_wnd_cell = self.wnd.pal_wnd.borrow();
        self.wnd.wm.set_wnd_attr(
            pal_wnd_cell.as_ref().unwrap(),
            pal::WndAttrs {
                listener: Some(Box::new(PalWndListener {
                    wnd: RcBorrow::to_weak(self.wnd),
                })),
                ..Default::default()
            },
        );

        // Raise `got_focus` if needed
        if self.wnd.wm.is_wnd_focused(pal_wnd_cell.as_ref().unwrap()) {
            self.invoke_focus_handlers();
        }
    }

    /// Pend an update.
    pub(super) fn pend_update(self) {
        if self.wnd.closed.get() {
            return;
        }

        // Already queued?
        let dirty = &self.wnd.dirty;
        if dirty.get().contains(WndDirtyFlags::UPDATE) {
            return;
        }
        dirty.set(dirty.get() | WndDirtyFlags::UPDATE);

        if let Some(ref pal_wnd) = *self.wnd.pal_wnd.borrow() {
            self.wnd.wm.request_update_ready_wnd(pal_wnd);
        } else {
            let hwnd: HWnd = self.cloned();
            self.wnd.wm.invoke(move |_| {
                hwnd.as_ref().update();
            });
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn invoke_on_next_frame_inner(
        self,
        f: Pin<Box<Node<AssertUnpin<dyn FnOnce(pal::Wm, HWndRef<'_>)>>>>,
    ) {
        if self.wnd.closed.get() {
            return;
        }

        let frame_handlers = &self.wnd.frame_handlers;

        if frame_handlers.is_empty() {
            if let Some(ref pal_wnd) = *self.wnd.pal_wnd.borrow() {
                self.wnd.wm.request_update_ready_wnd(pal_wnd);
            }
        }

        frame_handlers.push_back_node(f);
    }

    /// This is basically the handler of `update_ready` event and where layers
    /// are layouted and rendered. Also, the update process clears `Wnd::dirty`.
    fn update(self) {
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

        // Process `invoke_on_next_frame`.
        {
            let mut frame_handlers = self.wnd.frame_handlers.take();
            while let Some(cb) = frame_handlers.pop_front_node() {
                super::invocation::blackbox(move || {
                    (Pin::into_inner(cb).element.inner)(self.wnd.wm, self);
                });
            }
        }

        // They may set `CONTENTS`
        process_pending_invocations(self.wnd.wm);

        let update_contents = self.wnd.dirty.get().contains(WndDirtyFlags::CONTENTS);

        let RootSizeReq {
            new_size,
            min_size,
            max_size,
        } = if update_contents {
            self.update_views()
        } else {
            RootSizeReq::default()
        };

        // Clear the flag. Beyond this point, when `self.pend_update` is called,
        // a fresh update request will be enqueued.
        //
        // `self.pend_update` is called if a layout requests replacement of
        // layouts via `LayoutCtx::set_layout`. It's usually (i.e., if
        // `HView::set_layout` is called outside the layout process) okay, but
        // in this situation, we don't want a fresh update request to be
        // enqueued because `update_views` is designed to detect such a
        // situation by itself and restart the layout process. Thus, clearing
        // `UPDATE` must happen after `update_views`.
        {
            let dirty = &self.wnd.dirty;
            dirty.set(dirty.get() - WndDirtyFlags::UPDATE);
        }

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
        self.wnd.wm.set_wnd_attr(pal_wnd, attrs);

        // Un-suppress resize events
        self.wnd.updating.set(false);

        // Update layers
        if update_contents {
            self.wnd.wm.update_wnd(pal_wnd);
        }
    }

    /// Perform pending updates. Also, returns a new, min, and max window size
    /// based on the `SizeTraits` of the root view.
    fn update_views(self) -> RootSizeReq {
        let pal_wnd = self.wnd.pal_wnd.borrow();
        let pal_wnd = pal_wnd.as_ref().unwrap();

        let mut new_size = None;
        let mut min_size = None;
        let mut max_size = None;

        let resize_to_preferred = self.wnd.dirty.get().contains(WndDirtyFlags::DEFAULT_SIZE);

        // Repeat until the update converges...
        for _ in 0..100 {
            process_pending_invocations(self.wnd.wm);

            let view: HView = self.wnd.content_view.borrow().clone().unwrap();

            if !view.view.dirty.get().is_dirty() {
                return RootSizeReq {
                    new_size,
                    min_size,
                    max_size,
                };
            }

            view.as_ref().call_pending_mount_if_dirty(self.wnd.wm, self);

            // Layout: down phase
            view.as_ref().update_size_traits();

            // Constrain the window size
            let size_traits = view.view.size_traits.get();
            let wnd_size = if resize_to_preferred {
                [
                    size_traits.preferred.x as u32,
                    size_traits.preferred.y as u32,
                ]
            } else {
                self.wnd.wm.get_wnd_size(pal_wnd)
            };

            // A sensitive limit we set for window sizes.
            //
            // There used to be a backend that cannot handle a very value.
            // For instance, when dimensions close to `u32::max_value()` are
            // specified via `with_max_inner_size`, winit + Wayland (SCTK)
            // caused an error like "xdg_toplevel@31: error 4: invalid negative
            // max size requested -256 x -226". To be safe, we clamp the
            // maximum size by a much smaller value so that it won't cause a
            // problem.
            const SIZE_MAX: f32 = 16_777_216.0;

            let min_s = [
                size_traits.min.x.ceil() as u32,
                size_traits.min.y.ceil() as u32,
            ];
            let max_s = [
                size_traits.max.x.fmin(SIZE_MAX) as u32,
                size_traits.max.y.fmin(SIZE_MAX) as u32,
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
            let new_frame = box2! {
                min: [0.0, 0.0],
                max: [new_wnd_size[0] as f32, new_wnd_size[1] as f32],
            };
            if new_frame != view.view.frame.get() {
                view.view.frame.set(new_frame);
                view.view.global_frame.set(new_frame);
                view.as_ref()
                    .set_dirty_flags(ViewDirtyFlags::SUBVIEWS_FRAME);
            }

            // Layout: up phase
            view.as_ref().update_subview_frames();

            if view
                .view
                .dirty
                .get()
                .intersects(flags![ViewDirtyFlags::{SIZE_TRAITS | DESCENDANT_SIZE_TRAITS}])
            {
                // Some layout requested replacement of layouts.
                // Restart the layout process.
                continue;
            }

            // Position views
            view.as_ref().flush_position_event(self.wnd.wm);

            // Update visual
            view.as_ref().update_layers(self.wnd.wm, self);
        }

        panic!("Window update did not converge");
    }

    fn invoke_focus_handlers(self) {
        let handlers = self.wnd.focus_handlers.borrow();
        for handler in handlers.iter() {
            handler(self.wnd.wm, self);
        }

        // Raise `ViewListener::focus_(lost|leave|enter|got)` events
        self.raise_view_focus_events_for_wnd_focus_state_change();
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

            debug_assert!({
                // Get the superview of the content view
                let sv = &*view.view.superview.borrow();

                // ... which must be a window
                let wnd = sv.wnd().unwrap();

                // ... that is identical to `self`
                if let Some(wnd) = wnd.upgrade() {
                    std::ptr::eq(self, &*wnd)
                } else {
                    // This happens if `close` was called from `Wnd::drop`
                    true
                }
            });

            *view.view.superview.borrow_mut() = Superview::empty();

            view.as_ref().cancel_mouse_gestures_of_subviews(self);
            view.as_ref().call_unmount(self.wm);
        }

        if let Some(hwnd) = self.pal_wnd.borrow_mut().take() {
            self.wm.remove_wnd(&hwnd);
        }

        self.closed.set(true);
    }

    /// Set dirty flags on a window.
    pub(super) fn set_dirty_flags(&self, new_flags: WndDirtyFlags) {
        let dirty = &self.dirty;
        dirty.set(dirty.get() | new_flags);
    }

    pub(super) fn set_cursor_shape(&self, cursor_shape: CursorShape) {
        if cursor_shape == self.cursor_shape.get() {
            return;
        }
        self.cursor_shape.set(cursor_shape);

        let pal_wnd = self.pal_wnd.borrow();
        if let Some(ref pal_wnd) = *pal_wnd {
            self.wm.set_wnd_attr(
                pal_wnd,
                pal::WndAttrs {
                    cursor_shape: Some(cursor_shape),
                    ..Default::default()
                },
            )
        }
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

    fn invoke_later_with_hwnd(&self, wm: Wm, f: impl FnOnce(HWnd) + 'static) {
        use super::WmExt;

        let wnd = self.wnd.clone();
        wm.invoke_on_update(move |_| {
            if let Some(wnd) = wnd.upgrade() {
                f(HWnd { wnd });
            }
        });
    }
}

impl pal::iface::WndListener<Wm> for PalWndListener {
    fn close_requested(&self, wm: Wm, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            let hwnd = hwnd.as_ref();
            let listener = hwnd.wnd.listener.borrow();
            let should_close = listener.close_requested(wm, hwnd);
            if should_close {
                listener.close(wm, hwnd);
                hwnd.close();
            }
        }
    }

    fn update_ready(&self, _: Wm, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            hwnd.as_ref().update();
        }
    }

    fn resize(&self, _: Wm, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            let hwnd = hwnd.as_ref();

            if hwnd.wnd.updating.get() {
                // Prevent recursion
                return;
            }

            {
                let view = hwnd.wnd.content_view.borrow();
                let view = view.as_ref().unwrap();

                view.as_ref()
                    .set_dirty_flags(ViewDirtyFlags::SUBVIEWS_FRAME);
            }

            hwnd.wnd.set_dirty_flags(WndDirtyFlags::CONTENTS);

            // Layers should be updated *within* the call to thie method
            // for them to properly follow the window outline being dragged.
            hwnd.update();
        }
    }

    fn dpi_scale_changed(&self, _: Wm, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            let hwnd = hwnd.as_ref();
            let handlers = hwnd.wnd.dpi_scale_changed_handlers.borrow();
            for handler in handlers.iter() {
                handler(hwnd.wnd.wm, hwnd);
            }
        }
    }

    fn focus(&self, wm: Wm, _: &pal::HWnd) {
        // This handler can be called from `set_wnd_attrs`, which might conflict
        // with a mutable borrow for `style_attrs`
        self.invoke_later_with_hwnd(wm, |hwnd| {
            hwnd.as_ref().invoke_focus_handlers();
        });
    }

    fn key_down(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        e: &dyn pal::iface::KeyEvent<pal::AccelTable>,
    ) -> bool {
        if let Some(hwnd) = self.hwnd() {
            hwnd.as_ref().handle_key(e, false)
        } else {
            false
        }
    }

    fn key_up(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        e: &dyn pal::iface::KeyEvent<pal::AccelTable>,
    ) -> bool {
        if let Some(hwnd) = self.hwnd() {
            hwnd.as_ref().handle_key(e, true)
        } else {
            false
        }
    }

    fn interpret_event(
        &self,
        wm: Wm,
        _: &pal::HWnd,
        ctx: &mut dyn pal::iface::InterpretEventCtx<pal::AccelTable>,
    ) {
        if let Some(hwnd) = self.hwnd() {
            let hwnd = hwnd.as_ref();
            let listener = hwnd.wnd.listener.borrow();
            listener.interpret_event(wm, hwnd, ctx);
        }
    }

    fn validate_action(&self, _: Wm, _: &pal::HWnd, action: pal::ActionId) -> pal::ActionStatus {
        if let Some(hwnd) = self.hwnd() {
            hwnd.as_ref().handle_action(action, false)
        } else {
            pal::ActionStatus::empty()
        }
    }

    fn perform_action(&self, _: Wm, _: &pal::HWnd, action: pal::ActionId) {
        if let Some(hwnd) = self.hwnd() {
            hwnd.as_ref().handle_action(action, true);
        }
    }

    fn nc_hit_test(&self, _: Wm, _: &pal::HWnd, loc: Point2<f32>) -> pal::NcHit {
        if let Some(hwnd) = self.hwnd() {
            hwnd.handle_nc_hit_test(loc)
        } else {
            pal::NcHit::Client
        }
    }

    fn mouse_drag(
        &self,
        _: Wm,
        _: &pal::HWnd,
        loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn pal::iface::MouseDragListener<Wm>> {
        if let Some(hwnd) = self.hwnd() {
            hwnd.handle_mouse_drag(loc, button)
        } else {
            Box::new(())
        }
    }

    fn mouse_motion(&self, _: Wm, _: &pal::HWnd, loc: Point2<f32>) {
        if let Some(hwnd) = self.hwnd() {
            hwnd.handle_mouse_motion(Some(loc));
        }
    }

    fn mouse_leave(&self, _: Wm, _: &pal::HWnd) {
        if let Some(hwnd) = self.hwnd() {
            hwnd.handle_mouse_motion(None);
        }
    }

    fn scroll_motion(&self, _: Wm, _: &pal::HWnd, loc: Point2<f32>, delta: &pal::ScrollDelta) {
        if let Some(hwnd) = self.hwnd() {
            hwnd.handle_scroll_motion(loc, delta);
        }
    }

    fn scroll_gesture(
        &self,
        _: Wm,
        _: &pal::HWnd,
        loc: Point2<f32>,
    ) -> Box<dyn pal::iface::ScrollListener<Wm>> {
        if let Some(hwnd) = self.hwnd() {
            hwnd.handle_scroll_gesture(loc)
        } else {
            Box::new(())
        }
    }
}

pub(crate) fn new_root_content_view() -> HView {
    let view = HView::new(flags![ViewFlags::{LAYER_GROUP | CLIP_VISIBLE_FRAME}]);
    view.set_listener(RootViewListener::new());
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
    fn mount(&self, wm: Wm, _: HViewRef<'_>, _: HWndRef<'_>) {
        *self.layer.borrow_mut() = Some(wm.new_layer(pal::LayerAttrs {
            // `bounds` mustn't be empty, so...
            bounds: Some(box2! { min: [0.0, 0.0], max: [1.0, 1.0] }),
            ..Default::default()
        }));
    }

    fn unmount(&self, wm: Wm, _: HViewRef<'_>) {
        if let Some(hlayer) = self.layer.borrow_mut().take() {
            wm.remove_layer(&hlayer);
        }
    }

    fn update(&self, wm: Wm, _: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.borrow();
        let layer = layer.as_ref().unwrap();

        if let Some(sublayers) = ctx.sublayers().take() {
            wm.set_layer_attr(
                &layer,
                pal::LayerAttrs {
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
        const LAYER = 1;
        /// The window should be resized to the default size.
        const DEFAULT_SIZE = 1 << 1;

        const STYLE_VISIBLE = 1 << 2;
        const STYLE_FLAGS = 1 << 3;
        const STYLE_CAPTION = 1 << 4;

        const CONTENTS = 1 << 5;

        /// `update` is queued to the main event queue.
        const UPDATE = 1 << 6;
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
    fn transfer_to_pal<'a>(&'a self, dirty: WndDirtyFlags, attrs: &mut pal::WndAttrs<'a>) {
        if dirty.contains(WndDirtyFlags::STYLE_VISIBLE) {
            attrs.visible = Some(self.visible);
        }
        if dirty.contains(WndDirtyFlags::STYLE_FLAGS) {
            attrs.flags = Some(self.flags);
        }
        if dirty.contains(WndDirtyFlags::STYLE_CAPTION) {
            attrs.caption = Some(self.caption[..].into());
        }
    }
}
