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
    ui::{
        layouts::FillLayout,
        theming::{ClassSet, HElem, Manager, StyledBox, Widget},
    },
    uicore::{
        CursorShape, HView, HViewRef, Layout, LayoutCtx, MouseDragListener, SizeTraits, ViewFlags,
        ViewListener,
    },
};

const SPLITTER_TOLERANCE: f32 = 5.0;

/// A widget dividing a rectangle into two resizable panels.
///
/// # Rounding
///
/// The widget ensures the first panel's size is rounded to an integer.
/// Consequently, the same goes for the second panel, provided that the overall
/// size is an integer.
///
/// # Styling
///
///  - `parent > .SPLITTER` — The splitter element. The width is controlled by
///    `min_size`. The element has `.VERTICAL` if `vertical` is `true` (i.e.,
///    the region is separated by a horizontal line).
///
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
    fn down(&self, _: pal::Wm, _new_value: f32) {}

    /// The splitter is being moved. `new_value` specifies the new [`value`].
    ///
    /// The caller may return `new_value` as it is or return a modified `value`.
    ///
    /// [`value`]: crate::ui::views::split::Split::value
    fn motion(&self, _: pal::Wm, new_value: f32) -> f32 {
        new_value
    }

    /// The splitter was moved.
    fn up(&self, _: pal::Wm) {}

    /// The drag gesture was cancelled.
    fn cancel(&self, _: pal::Wm) {}
}

impl SplitDragListener for () {}

struct Shared {
    vertical: bool,
    fix: Option<u8>,
    value: Cell<f32>,
    zoom: Cell<Option<u8>>,
    container: HView,
    splitter: HView,
    splitter_sb: StyledBox,
    subviews: RefCell<[HView; 2]>,
    on_drag: RefCell<DragHandler>,
}

type DragHandler = Box<dyn Fn(pal::Wm) -> Box<dyn SplitDragListener>>;

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("vertical", &self.vertical)
            .field("fix", &self.fix)
            .field("value", &self.value)
            .field("zoom", &self.zoom)
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
    pub fn new(style_manager: &'static Manager, vertical: bool, fix: Option<usize>) -> Self {
        let container = HView::new(ViewFlags::default());
        let splitter = HView::new(
            ViewFlags::default() | ViewFlags::ACCEPT_MOUSE_DRAG | ViewFlags::ACCEPT_MOUSE_OVER,
        );

        let splitter_sb = StyledBox::new(style_manager, ViewFlags::default());

        splitter_sb.set_class_set(if vertical {
            ClassSet::SPLITTER | ClassSet::VERTICAL
        } else {
            ClassSet::SPLITTER
        });

        let shared = Rc::new(Shared {
            vertical,
            fix: match fix {
                None => None,
                Some(0) => Some(0),
                Some(1) => Some(1),
                _ => panic!("fix: index out of range"),
            },
            value: Cell::new(0.5),
            zoom: Cell::new(None),
            container: container.clone(),
            splitter: splitter.clone(),
            splitter_sb,
            // Fill the places with dummy views
            subviews: RefCell::new([
                HView::new(ViewFlags::default()),
                HView::new(ViewFlags::default()),
            ]),
            on_drag: RefCell::new(Box::new(|_| Box::new(()))),
        });

        splitter.set_listener(SplitterListener {
            shared: Rc::downgrade(&shared),
        });

        let mut margin = [0.0; 4];
        margin[!vertical as usize] = SPLITTER_TOLERANCE;
        margin[!vertical as usize + 2] = SPLITTER_TOLERANCE;
        splitter
            .set_layout(FillLayout::new(shared.splitter_sb.view().upgrade()).with_margin(margin));

        container.set_layout(shared.layout());

        Self { container, shared }
    }

    /// Get a handle to the view representing the widget.
    pub fn view(&self) -> HViewRef<'_> {
        self.container.as_ref()
    }

    /// Get the styling element of the splitter.
    pub fn style_elem(&self) -> HElem {
        self.shared.splitter_sb.style_elem()
    }

    /// Set the styling class set of the splitter.
    ///
    /// It defaults to `ClassSet::SPLITTER` or
    /// `ClassSet::SPLITTER | ClassSet::VERTICAL`.
    pub fn set_class_set(&self, class_set: ClassSet) {
        self.shared.splitter_sb.set_class_set(class_set);
    }

    /// Get the styling class set of the splitter.
    pub fn class_set(&self) -> ClassSet {
        self.shared.splitter_sb.class_set()
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
    pub fn set_value(&self, new_value: f32) {
        self.shared.set_value(new_value);
    }

    /// Set the panel to zoom into. Defaults to `None` (both panels are
    /// displayed). The value must be one of `Some(0)`, `Some(1)`, and `None`.
    pub fn set_zoom(&self, new_zoom: Option<u8>) {
        self.shared.set_zoom(new_zoom);
    }

    /// Set the views placed in the panels.
    pub fn set_subviews(&self, subviews: [HView; 2]) {
        *self.shared.subviews.borrow_mut() = subviews;
        self.shared.container.set_layout(self.shared.layout());
    }

    /// Set the factory function for gesture event handlers used when the user
    /// resizes the panels.
    ///
    /// The function is called when the user starts a mouse drag gesture.
    pub fn set_on_drag(&self, handler: impl Fn(pal::Wm) -> Box<dyn SplitDragListener> + 'static) {
        *self.shared.on_drag.borrow_mut() = Box::new(handler);
    }
}

impl Widget for Split {
    fn view(&self) -> HViewRef<'_> {
        self.view()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

impl Shared {
    fn splitter_width(&self) -> f32 {
        let axis_pri = self.vertical as usize;
        let size = self.splitter.frame().size()[axis_pri];
        size - SPLITTER_TOLERANCE * 2.0
    }

    /// Get `value` based on the actual position of the splitter.
    fn actual_value(&self) -> f32 {
        let axis_pri = self.vertical as usize;

        let size = self.container.frame().size()[axis_pri];
        let pos = self.splitter.frame().min[axis_pri] + SPLITTER_TOLERANCE;

        let splitter_width = self.splitter_width();

        match self.fix {
            None => pos / (size - splitter_width),
            Some(0) => pos,
            Some(1) => size - splitter_width - pos,
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

    fn set_zoom(&self, new_zoom: Option<u8>) {
        if new_zoom == self.zoom.get() {
            return;
        }
        self.zoom.set(new_zoom);

        self.container.set_layout(self.layout());
    }

    /// Calcuate the increase in `value` corresponding to a unit increase in
    /// X or Y coordinates.
    fn dvalue_dposition(&self) -> f32 {
        match self.fix {
            None => {
                let axis_pri = self.vertical as usize;
                let size = self.container.frame().size()[axis_pri];

                let splitter_width = self.splitter_width();

                1.0 / (size - splitter_width)
            }
            Some(0) => 1.0,
            Some(1) => -1.0,
            _ => unreachable!(),
        }
    }

    /// Construct a `Layout` based on the current state.
    fn layout(&self) -> Box<dyn Layout> {
        let subviews = self.subviews.borrow();

        if let Some(zoom) = self.zoom.get() {
            Box::new(FillLayout::new(subviews[zoom as usize].clone()))
        } else {
            Box::new(SplitLayout {
                vertical: self.vertical,
                fix: self.fix,
                value: self.value.get(),
                subviews: [
                    subviews[0].clone(),
                    subviews[1].clone(),
                    self.splitter.clone(),
                ],
            })
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
    splitter_width: f32,
) -> (f32, bool, bool) {
    let mut min = [st_min[0], size - splitter_width - st_max[1]].fmax();
    let mut max = [st_max[0], size - splitter_width - st_min[1]].fmin();

    // Make sure the first panel's size is rounded
    min = min.ceil();
    max = max.floor();

    let position = match fix {
        None => (size - splitter_width) * value,
        Some(0) => value,
        Some(1) => size - splitter_width - value,
        _ => unreachable!(),
    };

    let position = position.round().fmin(max).fmax(min);

    (position, position == min, position == max)
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
        let st1 = ctx.subview_size_traits(self.subviews[0].as_ref());
        let st2 = ctx.subview_size_traits(self.subviews[1].as_ref());
        let st_spl = ctx.subview_size_traits(self.subviews[2].as_ref());

        let axis_pri = self.vertical as usize;
        let axis_sec = axis_pri ^ 1;

        let splitter_width = st_spl.min[axis_pri] - SPLITTER_TOLERANCE * 2.0;

        let mut st = st1;
        let extra = splitter_width;
        st.min[axis_pri] = st1.min[axis_pri].ceil() + st2.min[axis_pri] + extra;
        st.max[axis_pri] = st1.max[axis_pri].floor() + st2.max[axis_pri] + extra;
        st.preferred[axis_pri] = st1.preferred[axis_pri] + st2.preferred[axis_pri] + extra;

        st.min[axis_sec] = [st1.min[axis_sec], st2.min[axis_sec], st_spl.min[axis_sec]].fmax();
        st.max[axis_sec] = [st1.max[axis_sec], st2.max[axis_sec], st_spl.max[axis_sec]].fmin();
        st.preferred[axis_sec] = ((st1.preferred[axis_sec] + st2.preferred[axis_sec]) * 0.5)
            .fmin(st.max[axis_sec])
            .fmax(st.min[axis_sec]);

        st
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        let st1 = ctx.subview_size_traits(self.subviews[0].as_ref());
        let st2 = ctx.subview_size_traits(self.subviews[1].as_ref());
        let st_spl = ctx.subview_size_traits(self.subviews[2].as_ref());

        let axis_pri = self.vertical as usize;

        let splitter_width = st_spl.min[axis_pri] - SPLITTER_TOLERANCE * 2.0;

        let (pos, at_min, at_max) = get_split_position(
            size[axis_pri],
            self.fix,
            self.value,
            [st1.min[axis_pri], st2.min[axis_pri]],
            [st1.max[axis_pri], st2.max[axis_pri]],
            splitter_width,
        );

        // Arrange the panels
        let mut frame1 = box2! { top_left: [0.0, 0.0], size: size };
        let mut frame2 = frame1;
        let mut spl_frame = frame1;

        frame1.max[axis_pri] = pos;
        frame2.min[axis_pri] = pos + splitter_width;

        ctx.set_subview_frame(self.subviews[0].as_ref(), frame1);
        ctx.set_subview_frame(self.subviews[1].as_ref(), frame2);

        // Arrange the splitter
        spl_frame.min[axis_pri] = pos - SPLITTER_TOLERANCE;
        spl_frame.max[axis_pri] = pos + splitter_width + SPLITTER_TOLERANCE;

        ctx.set_subview_frame(self.subviews[2].as_ref(), spl_frame);

        // Set the cursor shape. It's dependent on whether the splitter position
        // is at a limit or not.
        let shape_map = &[
            [
                // horizontal
                [CursorShape::EwResize, CursorShape::WResize],
                [CursorShape::EResize, CursorShape::Default],
            ],
            [
                // vertical
                [CursorShape::NsResize, CursorShape::NResize],
                [CursorShape::SResize, CursorShape::Default],
            ],
        ];
        self.subviews[2].set_cursor_shape(Some(
            shape_map[self.vertical as usize][at_min as usize][at_max as usize],
        ));
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
    shared: Weak<Shared>,
}

impl ViewListener for SplitterListener {
    fn mouse_drag(
        &self,
        wm: pal::Wm,
        _: HViewRef<'_>,
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
    fn mouse_down(&self, wm: pal::Wm, _: HViewRef<'_>, loc: Point2<f32>, button: u8) {
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

    fn mouse_motion(&self, wm: pal::Wm, _: HViewRef<'_>, loc: Point2<f32>) {
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

    fn mouse_up(&self, wm: pal::Wm, _: HViewRef<'_>, _loc: Point2<f32>, button: u8) {
        if button == 0 {
            *self.drag.borrow_mut() = None;

            self.user_listener.up(wm);
        }
    }

    fn cancel(&self, wm: pal::Wm, _: HViewRef<'_>) {
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
