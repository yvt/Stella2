//! Provides a widget dividing a rectangle into two resizable panels.
use alt_fp::{FloatOrd, FloatOrdSet};
use cggeom::{box2, prelude::*};
use cgmath::{Point2, Vector2};
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    pal,
    pal::prelude::*,
    uicore::{
        HView, HWnd, Layout, LayoutCtx, MouseDragListener, SizeTraits, UpdateCtx, ViewFlags,
        ViewListener,
    },
};

const SPLITTER_WIDTH: f32 = 1.0;
const SPLITTER_TOLERANCE: f32 = 5.0;

/// A widget dividing a rectangle into two resizable panels.
///
/// # Rounding
///
/// The widget ensures the first panel's size is rounded to an integer.
/// Consequently, the same goes for the second panel, provided that the overall
/// size is an integer.
#[derive(Debug)]
pub struct Split {
    container: HView,
    shared: Rc<Shared>,
}

/// Drag gesture handlers for [`Split`]. It has semantics similar to
/// `MouseDragListener`.
///
/// They are all called inside event handlers.
pub trait SplitDragListener {
    /// The splitter is about to be moved. `new_value` specifies the current,
    /// clipped [`value`].
    ///
    /// [`value`]: crate::ui::views::split::Split::value
    fn down(&self, _: pal::WM, _new_value: f32) {}

    /// The splitter is being moevd. `new_value` specifies the new [`value`].
    ///
    /// The caller may return `new_value` as it is or return a modified `value`.
    ///
    /// [`value`]: crate::ui::views::split::Split::value
    fn motion(&self, _: pal::WM, new_value: f32) -> f32 {
        new_value
    }

    /// The splitter was moved.
    ///
    /// [`value`]: crate::ui::views::split::Split::value
    fn up(&self, _: pal::WM) {}

    /// The drag gesture was cancelled.
    fn cancel(&self, _: pal::WM) {}
}

impl SplitDragListener for () {}

struct Shared {
    vertical: bool,
    fix: Option<u8>,
    value: Cell<f32>,
    container: HView,
    splitter: HView,
    subviews: RefCell<[HView; 2]>,
    on_drag: RefCell<Box<dyn Fn(pal::WM) -> Box<dyn SplitDragListener>>>,
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("vertical", &self.vertical)
            .field("fix", &self.fix)
            .field("value", &self.value)
            .field("container", &self.container)
            .field("splitter", &self.splitter)
            .field("subviews", &self.subviews)
            .field("on_drag", &())
            .finish()
    }
}

impl Split {
    /// Construct a new `Split`.
    ///
    /// `vertical` specifies the split direction. `fix` specifies which panel
    /// should be resized when the overall size is changed. It must be one of
    /// `Some(0)`, `Some(1)`, and `None`.
    pub fn new(vertical: bool, fix: Option<usize>) -> Self {
        let container = HView::new(ViewFlags::default());
        let splitter = HView::new(ViewFlags::default() | ViewFlags::ACCEPT_MOUSE_DRAG);

        let shared = Rc::new(Shared {
            vertical,
            fix: match fix {
                None => None,
                Some(0) => Some(0),
                Some(1) => Some(1),
                _ => panic!("fix: index out of range"),
            },
            value: Cell::new(0.5),
            container: container.clone(),
            splitter: splitter.clone(),
            // Fill the places with dummy views
            subviews: RefCell::new([
                HView::new(ViewFlags::default()),
                HView::new(ViewFlags::default()),
            ]),
            on_drag: RefCell::new(Box::new(|_| Box::new(()))),
        });

        splitter.set_listener(SplitterListener {
            vertical,
            shared: Rc::downgrade(&shared),
            layer: RefCell::new(None),
        });

        container.set_layout(shared.layout());

        // TODO: set cursor

        Self { container, shared }
    }

    /// Get a handle to the view representing the widget.
    pub fn view(&self) -> &HView {
        &self.container
    }

    /// Get a raw (unclipped) value representing the split position.
    ///
    /// The interpretation of this value differs depending on the value of `fix`
    /// specified when `new` is called. If `fix` is `Some(_)`, it represents the
    /// absolute size of the corresponding panel. Otherwise, it represents the
    /// percentage of the area occupied by the first panel.
    ///
    /// The returned value is raw and unclipped, meaning it does not take
    /// the size contraints of the panels into consideration. Also, it does not
    /// change when the overall size is changed.
    pub fn value(&self) -> f32 {
        self.shared.value.get()
    }

    /// Set the split position.
    ///
    /// See [`value`] for the interpretation of the value.
    ///
    /// [`value`]: self::Split::value
    pub fn set_value(&mut self, new_value: f32) {
        self.shared.set_value(new_value);
    }

    /// Set the views placed in the panels.
    pub fn set_subviews(&mut self, subviews: [HView; 2]) {
        *self.shared.subviews.borrow_mut() = subviews;
        self.shared.container.set_layout(self.shared.layout());
    }

    /// Set the factory function for gesture event handlers used when the user
    /// resizes the panels.
    ///
    /// The function is called when the user starts a mouse drag gesture.
    /// The `f32` parameter indicates the latest clipped `value`.
    pub fn set_on_drag(
        &mut self,
        handler: impl Fn(pal::WM) -> Box<dyn SplitDragListener> + 'static,
    ) {
        *self.shared.on_drag.borrow_mut() = Box::new(handler);
    }
}

impl Shared {
    /// Get `value` based on the actual position of the splitter.
    fn actual_value(&self) -> f32 {
        let axis_pri = self.vertical as usize;

        let size = self.container.frame().size()[axis_pri];
        let pos = self.splitter.frame().min[axis_pri] + SPLITTER_TOLERANCE;

        match self.fix {
            None => pos / (size - SPLITTER_WIDTH),
            Some(0) => pos,
            Some(1) => size - SPLITTER_WIDTH - pos,
            _ => unreachable!(),
        }
    }

    /// Set the split position, updating the layout as needed.
    fn set_value(&self, new_value: f32) {
        if new_value == self.value.get() {
            return;
        }
        self.value.set(new_value);

        self.container.set_layout(self.layout());
    }

    /// Calcuate the increase in `value` corresponding to a unit increase in
    /// X or Y coordinates.
    fn dvalue_dposition(&self) -> f32 {
        match self.fix {
            None => {
                let axis_pri = self.vertical as usize;
                let size = self.container.frame().size()[axis_pri];

                1.0 / (size - SPLITTER_WIDTH)
            }
            Some(0) => 1.0,
            Some(1) => -1.0,
            _ => unreachable!(),
        }
    }

    /// Construct a `Layout` based on the current state.
    fn layout(&self) -> SplitLayout {
        let subviews = self.subviews.borrow();
        SplitLayout {
            vertical: self.vertical,
            fix: self.fix,
            value: self.value.get(),
            subviews: [
                subviews[0].clone(),
                subviews[1].clone(),
                self.splitter.clone(),
            ],
        }
    }
}

/// Calculate the split position (the size of the first panel).
fn get_split_position(
    size: f32,
    fix: Option<u8>,
    value: f32,
    st_min: [f32; 2],
    st_max: [f32; 2],
) -> f32 {
    let mut min = [st_min[0], size - SPLITTER_WIDTH - st_max[0]].fmax();
    let mut max = [st_max[0], size - SPLITTER_WIDTH - st_min[0]].fmin();

    // Make sure the first panel's size is rounded
    min = min.ceil();
    max = max.floor();

    let position = match fix {
        None => (size - SPLITTER_WIDTH) * value,
        Some(0) => value,
        Some(1) => size - SPLITTER_WIDTH - value,
        _ => unreachable!(),
    };

    position.fmin(max).fmax(min)
}

#[derive(Debug)]
struct SplitLayout {
    vertical: bool,
    fix: Option<u8>,
    value: f32,
    subviews: [HView; 3],
}

impl Layout for SplitLayout {
    fn subviews(&self) -> &[HView] {
        &self.subviews
    }

    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits {
        let st1 = ctx.subview_size_traits(&self.subviews[0]);
        let st2 = ctx.subview_size_traits(&self.subviews[1]);

        let axis_pri = self.vertical as usize;
        let axis_sec = axis_pri ^ 1;

        let mut st = st1;
        let extra = SPLITTER_WIDTH;
        st.min[axis_pri] = st1.min[axis_pri].ceil() + st2.min[axis_pri] + extra;
        st.max[axis_pri] = st1.max[axis_pri].floor() + st2.max[axis_pri] + extra;
        st.preferred[axis_pri] = st1.preferred[axis_pri] + st2.preferred[axis_pri] + extra;

        st.min[axis_sec] = [st1.min[axis_sec], st2.min[axis_sec]].fmax();
        st.max[axis_sec] = [st1.max[axis_sec], st2.max[axis_sec]].fmin();
        st.preferred[axis_sec] = ((st1.preferred[axis_sec] + st2.preferred[axis_sec]) * 0.5)
            .fmin(st.max[axis_sec])
            .fmax(st.min[axis_sec]);

        st
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        let st1 = ctx.subview_size_traits(&self.subviews[0]);
        let st2 = ctx.subview_size_traits(&self.subviews[1]);

        let axis_pri = self.vertical as usize;

        let pos = get_split_position(
            size[axis_pri],
            self.fix,
            self.value,
            [st1.min[axis_pri], st2.min[axis_pri]],
            [st1.max[axis_pri], st2.max[axis_pri]],
        );

        // Arrange the panels
        let mut frame1 = box2! { top_left: [0.0, 0.0], size: size };
        let mut frame2 = frame1;
        let mut spl_frame = frame1;

        frame1.max[axis_pri] = pos;
        frame2.min[axis_pri] = pos + SPLITTER_WIDTH;

        ctx.set_subview_frame(&self.subviews[0], frame1);
        ctx.set_subview_frame(&self.subviews[1], frame2);

        // Arrange the splitter
        spl_frame.min[axis_pri] = pos - SPLITTER_TOLERANCE;
        spl_frame.max[axis_pri] = pos + SPLITTER_WIDTH + SPLITTER_TOLERANCE;

        ctx.set_subview_frame(&self.subviews[2], spl_frame);
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        if let Some(other) = as_any::Downcast::downcast_ref::<Self>(other) {
            self.subviews == other.subviews
        } else {
            false
        }
    }
}

struct SplitterListener {
    vertical: bool,
    shared: Weak<Shared>,
    layer: RefCell<Option<pal::HLayer>>,
}

impl ViewListener for SplitterListener {
    fn mount(&self, wm: pal::WM, view: &HView, _: &HWnd) {
        // Create a layer for the splitter line
        let layer = wm.new_layer(pal::LayerAttrs {
            bg_color: Some(pal::RGBAF32::new(0.1, 0.1, 0.1, 1.0)),
            ..Default::default()
        });

        *self.layer.borrow_mut() = Some(layer);

        view.pend_update();
    }

    fn unmount(&self, wm: pal::WM, _: &HView) {
        let layer = self.layer.borrow_mut().take().unwrap();
        wm.remove_layer(&layer);
    }

    fn position(&self, _: pal::WM, view: &HView) {
        view.pend_update();
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.borrow();
        let layer = layer.as_ref().unwrap();

        let mut frame = view.global_frame();

        let axis_pri = self.vertical as usize;
        frame.min[axis_pri] += SPLITTER_TOLERANCE;
        frame.max[axis_pri] -= SPLITTER_TOLERANCE;

        wm.set_layer_attr(
            layer,
            pal::LayerAttrs {
                bounds: Some(frame),
                ..Default::default()
            },
        );

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![layer.clone()]);
        }
    }

    fn mouse_drag(
        &self,
        wm: pal::WM,
        _: &HView,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn MouseDragListener> {
        if let Some(shared) = self.shared.upgrade() {
            let on_drag = shared.on_drag.borrow();

            let user_listener = on_drag(wm);

            Box::new(SplitterDragListener {
                shared: Weak::clone(&self.shared),
                drag: RefCell::new(None),
                orig_value: shared.value.get(),
                user_listener,
            })
        } else {
            Box::new(())
        }
    }
}

struct SplitterDragListener {
    shared: Weak<Shared>,
    drag: RefCell<Option<DragState>>,
    orig_value: f32,
    user_listener: Box<dyn SplitDragListener>,
}

#[derive(Clone)]
struct DragState {
    start_value: f32,
    start_mouse_loc: f32,
}

impl MouseDragListener for SplitterDragListener {
    fn mouse_down(&self, wm: pal::WM, _: &HView, loc: Point2<f32>, button: u8) {
        if let Some(shared) = self.shared.upgrade() {
            if button == 0 {
                let axis_pri = shared.vertical as usize;

                let actual_value = shared.actual_value();

                *self.drag.borrow_mut() = Some(DragState {
                    start_value: actual_value,
                    start_mouse_loc: loc[axis_pri],
                });

                self.user_listener.down(wm, actual_value);
            }
        }
    }

    fn mouse_motion(&self, wm: pal::WM, _: &HView, loc: Point2<f32>) {
        if let (Some(shared), Some(drag)) = (self.shared.upgrade(), self.drag.borrow().clone()) {
            let axis_pri = shared.vertical as usize;

            let dfdx = shared.dvalue_dposition();
            let new_value = drag.start_value + (loc[axis_pri] - drag.start_mouse_loc) * dfdx;

            let new_value = self.user_listener.motion(wm, new_value);

            wm.invoke(move |_| {
                shared.set_value(new_value);
            });
        }
    }

    fn mouse_up(&self, wm: pal::WM, _: &HView, _loc: Point2<f32>, button: u8) {
        if button == 0 {
            *self.drag.borrow_mut() = None;

            self.user_listener.up(wm);
        }
    }

    fn cancel(&self, wm: pal::WM, _: &HView) {
        if let Some(shared) = self.shared.upgrade() {
            self.user_listener.cancel(wm);

            // Restore the original value
            let orig_value = self.orig_value;

            wm.invoke(move |_| {
                shared.set_value(orig_value);
            });
        }
    }
}
