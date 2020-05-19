//! Implements the slider.
use alt_fp::FloatOrd;
use cggeom::{box2, prelude::*};
use cgmath::Point2;
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
};

use crate::{
    pal,
    prelude::*,
    ui::{
        layouts::{EmptyLayout, FillLayout},
        mixins::CanvasMixin,
        theming,
        theming::{
            elem_id, roles, ClassSet, GetPropValue, HElem, Manager, ModifyArrangementArgs,
            PropKindFlags, Role, StyledBox, StyledBoxOverride, Widget,
        },
    },
    uicore::{HView, HViewRef, HWndRef, MouseDragListener, UpdateCtx, ViewFlags, ViewListener},
    utils::resetiter,
};

// Reuse some items from the scrollbar implementation
use super::scrollbar::ListenerOnUpdateFilter;
#[doc(no_inline)]
pub use super::scrollbar::{Dir, ScrollbarDragListener};

/// A slider widget.
///
/// # Custom Label Views
///
/// A slider can have *custom label views* tethered to specific values. They can
/// be set by calling [`Slider::set_labels`].
///
/// *Performance notes:* Custom label views use O(n²) algorithms in many of
/// their code paths. Please keep the number of views low (< 8).
///
/// # Styling
///
///  - `style_elem` - See [`StyledBox`](crate::ui::theming::StyledBox)
///     - `subviews[`[`roles::SLIDER_LABELS`]`]`: The wrapper for custom label
///       views.
///
///     - `subviews[`[`roles::SLIDER_KNOB`]`]`: The knob. `Slider` overrides the
///       knob's `frame` using the current value. The original `frame`
///       represents the knob's movable range. The size along the primary axis
///       is always set to minimum.
///
///       *Note:* "The original `frame`" is the initial `frame` calculated by
///       `StyledBox`'s layout algorithm and is bounded by the subview's maximum
///       size. The overall size of `Slider` will be affected as normally it
///       would. You need to make sure the maximum size is set to infinity to
///       achieve a desired effect.
///
///     - `subviews[`[`roles::SLIDER_TICKS`]`]`: The container for tick marks.
///       Should align with the movable range of the knob for it to make sense
///       to the application user.
///
///  - `style_elem > #`[`SLIDER_KNOB`] - The knob. See
///    [`StyledBox`](crate::ui::theming::StyledBox)
///
///  - `style_elem > #`[`SLIDER_TICKS`] - The ticks. Supports `FgColor`.
///
///  - `style_elem > #`[`SLIDER_LABELS`] - The wrapper for custom label views.
///     - `subviews[role]`: The custom label view with a role `role`.
///       The original `frame` represents the value range. The size along
///       the primary axis is always set to minimum.
///
///  - `style_elem > #SLIDER_LABELS > *` - Custom label views.
///
/// [`roles::SLIDER_KNOB`]: crate::ui::theming::roles::SLIDER_KNOB
/// [`roles::SLIDER_TICKS`]: crate::ui::theming::roles::SLIDER_TICKS
/// [`roles::SLIDER_LABELS`]: crate::ui::theming::roles::SLIDER_LABELS
/// [`SLIDER_KNOB`]: crate::ui::theming::elem_id::SLIDER_KNOB
/// [`SLIDER_TICKS`]: crate::ui::theming::elem_id::SLIDER_TICKS
/// [`SLIDER_LABELS`]: crate::ui::theming::elem_id::SLIDER_LABELS
///
#[derive(Debug)]
pub struct Slider {
    shared: Rc<Shared>,
}

struct Shared {
    vertical: bool,
    value: Cell<f64>,
    on_drag: RefCell<DragHandler>,
    on_step: RefCell<StepHandler>,
    wrapper: HView,
    frame: StyledBox,
    knob: StyledBox,
    layout_state: Cell<LayoutState>,

    ticks_view: HView,
    ticks_elem: theming::Elem,
    ticks_state: Rc<RefCell<TicksState>>,
    ticks_empty: Cell<bool>,

    labels_wrapper: StyledBox,
    labels: RefCell<Vec<Label>>,
}

struct Label {
    role: Role,
    value: f32,
    /// Wraps the given view to make sure `SizeTraits::max` is infinity.
    /// (We don't want the slider's maximum size limited by the labels within)
    wrap_view: HView,
}

type DragHandler = Box<dyn Fn(pal::Wm) -> Box<dyn ScrollbarDragListener>>;
type StepHandler = Box<dyn Fn(pal::Wm, Dir)>;

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("vertical", &self.vertical)
            .field("value", &self.value)
            .field("on_drag", &())
            .field("on_step", &())
            .field("frame", &self.frame)
            .field("knob", &self.knob)
            .field("layout_state", &self.layout_state)
            .finish()
    }
}

/// Information obtained from the actual geometry of the slider's elements.
#[derive(Copy, Clone, Debug, Default)]
struct LayoutState {
    knob_start: f32,
    knob_end: f32,
    /// The left/top local coordinate of the range in which the origin point of
    /// the knob can move.
    knob_origin_start: f64,
    clearance: f64,
}

impl Slider {
    pub fn new(style_manager: &'static Manager, vertical: bool) -> Self {
        let frame = StyledBox::new(
            style_manager,
            ViewFlags::ACCEPT_MOUSE_OVER | ViewFlags::TAB_STOP,
        );
        frame.set_class_set(if vertical {
            ClassSet::SLIDER | ClassSet::VERTICAL
        } else {
            ClassSet::SLIDER
        });
        frame.set_auto_class_set(ClassSet::HOVER | ClassSet::FOCUS);

        let wrapper = HView::new(ViewFlags::ACCEPT_MOUSE_DRAG);
        wrapper.set_layout(FillLayout::new(frame.view()));

        // Tick marks
        let ticks_view = HView::new(ViewFlags::default());
        let ticks_elem = theming::Elem::new(style_manager);
        ticks_elem.set_class_set(elem_id::SLIDER_TICKS);
        let ticks_state = Rc::new(RefCell::new(TicksState {
            canvas: CanvasMixin::new(),
            values: Box::new(resetiter::empty()),
        }));
        frame.set_subelement(roles::SLIDER_TICKS, Some(ticks_elem.helem()));

        // Knob
        let knob = StyledBox::new(style_manager, ViewFlags::default());
        knob.set_class_set(elem_id::SLIDER_KNOB);
        frame.set_child(roles::SLIDER_KNOB, Some(&knob));

        // Wrapper for custom labels
        let labels_wrapper = StyledBox::new(style_manager, ViewFlags::default());
        labels_wrapper.set_class_set(elem_id::SLIDER_LABELS);

        let shared = Rc::new(Shared {
            vertical,
            value: Cell::new(0.0),
            on_drag: RefCell::new(Box::new(|_| Box::new(()))),
            on_step: RefCell::new(Box::new(|_, _| {})),
            wrapper,
            frame,
            knob,
            layout_state: Cell::new(LayoutState::default()),
            ticks_view,
            ticks_elem,
            ticks_state,
            ticks_empty: Cell::new(true),
            labels_wrapper,
            labels: RefCell::new(Vec::new()),
        });

        Shared::update_sb_override(&shared);

        shared.wrapper.set_listener(SlViewListener {
            shared: Rc::downgrade(&shared),
        });

        shared.ticks_view.set_listener(TicksViewListener {
            shared: Rc::downgrade(&shared),
            state: Rc::clone(&shared.ticks_state),
        });

        Self { shared }
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::SLIDER`. Some bits (e.g., `ACTIVE`) are
    /// internally enforced and cannot be modified.
    pub fn set_class_set(&self, mut class_set: ClassSet) {
        let frame = &self.shared.frame;

        // Protected bits
        let protected = ClassSet::ACTIVE;
        class_set -= protected;
        class_set |= frame.class_set() & protected;
        frame.set_class_set(class_set);
    }

    /// Get the class set of the inner `StyledBox`.
    pub fn class_set(&self) -> ClassSet {
        self.shared.frame.class_set()
    }

    /// Get the current value.
    pub fn value(&self) -> f64 {
        self.shared.value.get()
    }

    /// Set the current value in range `[0, 1]`.
    pub fn set_value(&self, new_value: f64) {
        debug_assert!(new_value >= 0.0, "{} >= 0.0", new_value);
        debug_assert!(new_value <= 1.0, "{} <= 1.0", new_value);

        if new_value == self.shared.value.get() {
            return;
        }

        self.shared.value.set(new_value);
        Shared::update_sb_override(&self.shared);
    }

    /// Set the tick mark positions.
    pub fn set_ticks<I>(&self, new_ticks: I)
    where
        I: resetiter::IntoResetIter<Item = f64>,
        I::IntoResetIter: 'static,
    {
        self.set_ticks_inner(Box::new(new_ticks.into_reset_iter()));
    }

    fn set_ticks_inner(&self, mut new_ticks: Box<dyn resetiter::ResetIter<Item = f64>>) {
        let is_empty = new_ticks.is_empty();

        let mut ticks_state = self.shared.ticks_state.borrow_mut();
        ticks_state.values = new_ticks;

        if is_empty != self.shared.ticks_empty.get() {
            self.shared.ticks_empty.set(is_empty);
            self.shared.frame.set_subview(
                roles::SLIDER_TICKS,
                if is_empty {
                    None
                } else {
                    Some(self.shared.ticks_view.clone())
                },
            );
        }

        if !is_empty {
            ticks_state
                .canvas
                .pend_draw(self.shared.ticks_view.as_ref());
        }
    }

    /// Arrange tick marks uniformly by calling `set_ticks`.
    ///
    /// `num_segments` must not be zero.
    pub fn set_uniform_ticks(&self, num_segments: usize) {
        debug_assert!(num_segments != 0);
        let num_segments_f64 = num_segments as f64;
        self.set_ticks(
            (0..=num_segments)
                .into_reset_iter()
                .map(move |i| i as f64 / num_segments_f64),
        );
    }

    /// Set custom label views attached to specified values.
    pub fn set_labels<'a>(&self, children: impl AsRef<[(Role, Option<(f64, &'a dyn Widget)>)]>) {
        self.set_labels_inner(children.as_ref());
    }

    fn set_labels_inner(&self, children: &[(Role, Option<(f64, &dyn Widget)>)]) {
        let mut labels = self.shared.labels.borrow_mut();
        let labels_wrapper = &self.shared.labels_wrapper;

        // `wrap_view` has flexible margin values so that the slider's maximum
        // size is not constrained by individual custom labels.
        use std::f32::NAN;
        let wrap_view_margin = [
            // Horizontal - the top edge of the label is fixed
            [0.0, NAN, NAN, NAN],
            // Vertical - the left edge of the label is fixed
            [NAN, NAN, NAN, 0.0],
        ][self.shared.vertical as usize];

        for &(role, label) in children.iter() {
            if let Some((new_value, new_widget)) = label {
                let new_value = new_value as f32;
                let wrap_view =
                    if let Some(label) = labels.iter_mut().find(|label| label.role == role) {
                        // Replace the existing label
                        label.value = new_value;
                        &label.wrap_view
                    } else {
                        // Add a new label
                        let wrap_view = HView::new(ViewFlags::default());
                        labels_wrapper.set_subview(role, Some(wrap_view.clone()));
                        labels.push(Label {
                            role,
                            wrap_view,
                            value: new_value,
                        });
                        &labels.last_mut().unwrap().wrap_view
                    };
                wrap_view.set_layout(
                    FillLayout::new(new_widget.view_ref().cloned()).with_margin(wrap_view_margin),
                );
            } else {
                if let Some(i) = labels.iter().position(|label| label.role == role) {
                    // Remove the label
                    let label = labels.swap_remove(i);
                    debug_assert_eq!(label.role, role);
                    labels_wrapper.set_subview(role, None);
                    label
                        .wrap_view
                        .set_layout(EmptyLayout::new(Default::default()));
                }
            }

            labels_wrapper.set_subelement(role, label.and_then(|(_, widget)| widget.style_elem()));
        }

        if labels.is_empty() {
            self.shared.frame.set_child(roles::SLIDER_LABELS, None);
        } else {
            // Display `SLIDER_LABELS` only if there's a label. This way, the
            // slider will have an extra size only when necessary.
            self.shared
                .frame
                .set_child(roles::SLIDER_LABELS, Some(labels_wrapper));

            labels_wrapper.set_override(LabelsStyledBoxOverride {
                vertical: self.shared.vertical,
                labels: labels
                    .iter()
                    .map(|label| (label.role, label.value))
                    .collect(),
            });
        }
    }

    /// Set the factory function for gesture event handlers used when the user
    /// grabs the knob.
    ///
    /// The function is called when the user starts a mouse drag gesture.
    pub fn set_on_drag(
        &self,
        handler: impl Fn(pal::Wm) -> Box<dyn ScrollbarDragListener> + 'static,
    ) {
        *self.shared.on_drag.borrow_mut() = Box::new(handler);
    }

    /// Set the handler function called when the user hits an arrow key to
    /// manipulate the slider.
    ///
    /// The function is called through `invoke_on_update`.
    pub fn set_on_step(&self, handler: impl Fn(pal::Wm, Dir) + 'static) {
        *self.shared.on_step.borrow_mut() = Box::new(handler);
    }

    /// Get an owned handle to the view representing the widget.
    pub fn view(&self) -> HView {
        self.shared.wrapper.clone()
    }

    /// Borrow the handle to the view representing the widget.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.shared.wrapper.as_ref()
    }

    /// Get the styling element representing the widget.
    pub fn style_elem(&self) -> HElem {
        self.shared.frame.style_elem()
    }
}

impl Widget for Slider {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

impl Shared {
    fn update_sb_override(this: &Rc<Shared>) {
        this.frame.set_override(SlStyledBoxOverride {
            value: this.value.get(),
            shared: Rc::downgrade(this),
        })
    }

    fn set_active(&self, active: bool) {
        let frame = &self.frame;

        let mut class_set = frame.class_set();
        class_set.set(ClassSet::ACTIVE, active);
        frame.set_class_set(class_set);
    }
}

/// Implements `StyledBoxOverride` for `Slider`.
struct SlStyledBoxOverride {
    value: f64,
    /// This reference to `Shared` is used to provide layout feedback. The above
    /// fields should remain to ensure the logical immutability of this
    /// `StyledBoxOverride`. (This is actually never a problem in the current
    /// implementation of `StyledBox`, though.)
    shared: Weak<Shared>,
}

impl StyledBoxOverride for SlStyledBoxOverride {
    fn modify_arrangement(
        &self,
        ModifyArrangementArgs {
            size_traits,
            frame,
            role,
            ..
        }: ModifyArrangementArgs<'_>,
    ) {
        match role {
            roles::SLIDER_TICKS | roles::SLIDER_LABELS => {
                // Do not modify the arrangement of these subviews.
            }
            roles::SLIDER_KNOB => {
                let shared = if let Some(shared) = self.shared.upgrade() {
                    shared
                } else {
                    return;
                };

                let pri = shared.vertical as usize;

                let bar_len = frame.size()[pri] as f64;
                let bar_start = frame.min[pri] as f64;

                let knob_len = size_traits.min[pri] as f64;
                let clearance = bar_len - knob_len;

                let knob_origin_start = bar_start + knob_len * 0.5;

                let knob_start = bar_start + self.value * clearance;
                let knob_end = knob_start + knob_len;
                frame.min[pri] = knob_start as f32;
                frame.max[pri] = knob_end as f32;

                // Layout feedback
                shared.layout_state.set(LayoutState {
                    knob_start: knob_start as f32,
                    knob_end: knob_end as f32,
                    clearance,
                    knob_origin_start,
                });
            }
            _ => unreachable!(),
        }
    }

    fn dirty_flags(&self, other: &dyn StyledBoxOverride) -> PropKindFlags {
        use as_any::Downcast;
        if let Some(other) = (*other).downcast_ref::<Self>() {
            if self.value == other.value {
                PropKindFlags::empty()
            } else {
                PropKindFlags::LAYOUT
            }
        } else {
            PropKindFlags::all()
        }
    }
}

/// Implements `StyledBoxOverride` for custom label views.
struct LabelsStyledBoxOverride {
    vertical: bool,
    labels: Vec<(Role, f32)>,
}

impl StyledBoxOverride for LabelsStyledBoxOverride {
    fn modify_arrangement(
        &self,
        ModifyArrangementArgs {
            size_traits,
            frame,
            role,
            ..
        }: ModifyArrangementArgs<'_>,
    ) {
        let pri = self.vertical as usize;

        let value = self
            .labels
            .iter()
            .find(|&&(r, _)| r == role)
            .map(|&(_, value)| value)
            .unwrap_or(0.0);

        let label_size = size_traits.min[pri];
        let label_pos = frame.min[pri] + (frame.max[pri] - frame.min[pri]) * value;
        frame.min[pri] = label_pos - label_size / 2.0;
        frame.max[pri] = label_pos + label_size / 2.0;
    }

    fn dirty_flags(&self, other: &dyn StyledBoxOverride) -> PropKindFlags {
        use as_any::Downcast;
        if let Some(other) = (*other).downcast_ref::<Self>() {
            if self.labels == other.labels {
                PropKindFlags::empty()
            } else {
                PropKindFlags::LAYOUT
            }
        } else {
            PropKindFlags::all()
        }
    }
}

/// Implements `ViewListener` for `Slider`.
struct SlViewListener {
    shared: Weak<Shared>,
}

impl ViewListener for SlViewListener {
    fn mouse_drag(
        &self,
        _: pal::Wm,
        _: HViewRef<'_>,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn MouseDragListener> {
        if let Some(shared) = self.shared.upgrade() {
            Box::new(SlMouseDragListener {
                shared,
                drag_start: Cell::new(None),
                listener: RefCell::new(None),
            })
        } else {
            Box::new(())
        }
    }
}

/// Implements `MouseDragListener` for `Slider`.
struct SlMouseDragListener {
    shared: Rc<Shared>,
    drag_start: Cell<Option<(f32, f64)>>,
    listener: RefCell<Option<ListenerOnUpdateFilter>>,
}

impl MouseDragListener for SlMouseDragListener {
    fn mouse_motion(&self, wm: pal::Wm, _: HViewRef<'_>, loc: Point2<f32>) {
        if let Some((init_pos, init_value)) = self.drag_start.get() {
            let pri = self.shared.vertical as usize;
            let clearance = self.shared.layout_state.get().clearance;

            if clearance == 0.0 {
                return;
            }

            let new_value = (init_value + (loc[pri] - init_pos) as f64 / clearance)
                .fmax(0.0)
                .fmin(1.0);

            let listener = self.listener.borrow();
            if let Some(listener) = &*listener {
                listener.motion(wm, new_value);
            }
        }
    }
    fn mouse_down(&self, wm: pal::Wm, view: HViewRef<'_>, loc: Point2<f32>, button: u8) {
        if button == 0 {
            let pri = self.shared.vertical as usize;
            let loc = loc[pri];

            // Detect trough clicking
            let layout_state = self.shared.layout_state.get();
            let local_loc = loc - view.global_frame().min[pri];

            let on_knob =
                local_loc >= layout_state.knob_start && local_loc <= layout_state.knob_end;

            if on_knob {
                self.drag_start.set(Some((loc, self.shared.value.get())));
            } else {
                // Jump to the clicked point if `on_knob == false`
                let knob_origin_start = layout_state.knob_origin_start;
                let value = ((local_loc as f64 - knob_origin_start) / layout_state.clearance)
                    .fmax(0.0)
                    .fmin(1.0);
                self.drag_start.set(Some((loc, value)));
            }

            self.shared.set_active(true);

            if self.listener.borrow().is_none() {
                let listener = self.shared.on_drag.borrow()(wm);
                let listener = ListenerOnUpdateFilter::new(listener);
                *self.listener.borrow_mut() = Some(listener);
            }

            (self.listener.borrow().as_ref().unwrap()).down(wm, self.shared.value.get());

            // Jump to the clicked point if `on_knob == false`
            if !on_knob {
                if let (Some(listener), Some((_, init_value))) =
                    (self.listener.borrow().as_ref(), self.drag_start.get())
                {
                    listener.motion(wm, init_value);
                }
            }
        }
    }
    fn mouse_up(&self, wm: pal::Wm, _: HViewRef<'_>, _loc: Point2<f32>, button: u8) {
        if button == 0 && self.drag_start.take().is_some() {
            self.shared.set_active(false);
            self.listener.borrow().as_ref().unwrap().up(wm);
        }
    }
    fn cancel(&self, wm: pal::Wm, _: HViewRef<'_>) {
        if self.drag_start.take().is_some() {
            self.shared.set_active(false);
        }
        self.listener.borrow().as_ref().unwrap().cancel(wm);
    }
}

/// Implements `ViewListener` for the view displaying tick marks.
struct TicksViewListener {
    shared: Weak<Shared>,
    state: Rc<RefCell<TicksState>>,
}

struct TicksState {
    canvas: CanvasMixin,
    values: Box<dyn resetiter::ResetIter<Item = f64>>,
}

impl ViewListener for TicksViewListener {
    fn mount(&self, wm: pal::Wm, view: HViewRef<'_>, wnd: HWndRef<'_>) {
        self.state.borrow_mut().canvas.mount(wm, view, wnd);
    }

    fn unmount(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.state.borrow_mut().canvas.unmount(wm, view);
    }

    fn position(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.state.borrow_mut().canvas.position(wm, view);
    }

    fn update(&self, wm: pal::Wm, view: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // enable split borrow

        // Make the visual bounds slightly larger than the frame so that
        // the tickmarks at `0` and `1` are not clipped
        let size = view.frame().size();
        let visual_bounds = box2! {
            top_left: [-1.0, 0.0],
            size: [size.x + 2.0, size.y]
        };

        // The tick marks' color
        let color = if let Some(shared) = self.shared.upgrade() {
            shared.ticks_elem.computed_values().fg_color()
        } else {
            [0.0, 0.0, 0.0, 0.0].into()
        };

        // Render all tick marks in a single layer
        let values = &mut *state.values;
        state
            .canvas
            .update_layer(wm, view, ctx.hwnd(), visual_bounds, move |draw_ctx| {
                let c = &mut draw_ctx.canvas;

                c.set_stroke_rgb(color);
                c.begin_path();

                for x in values.iter().map(|x| x as f32 * size.x) {
                    c.move_to([x, 0.0].into());
                    c.line_to([x, size.y].into());
                }

                c.stroke();
            });

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![state.canvas.layer().unwrap().clone()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use cgmath::assert_abs_diff_eq;
    use enclose::enc;
    use log::{debug, info};
    use std::rc::Weak;
    use try_match::try_match;

    use super::*;
    use crate::{
        pal,
        testing::{prelude::*, use_testing_wm},
        ui::layouts::FillLayout,
        uicore::HWnd,
    };

    trait Transpose: Sized {
        fn t(self) -> Self;
        fn t_if(self, cond: bool) -> Self {
            if cond {
                self.t()
            } else {
                self
            }
        }
    }

    impl<T> Transpose for [T; 2] {
        fn t(self) -> Self {
            let [x, y] = self;
            [y, x]
        }
    }

    impl<T> Transpose for Point2<T> {
        fn t(self) -> Self {
            let Self { x: y, y: x } = self;
            Self { x, y }
        }
    }

    impl<T> Transpose for cggeom::Box2<T> {
        fn t(self) -> Self {
            Self {
                min: self.min.t(),
                max: self.max.t(),
            }
        }
    }

    fn make_wnd(twm: &dyn TestingWm, vertical: bool) -> (Rc<Slider>, HWnd, pal::HWnd) {
        let wm = twm.wm();

        let style_manager = Manager::global(wm);
        let sb = Rc::new(Slider::new(style_manager, vertical));

        let wnd = HWnd::new(wm);
        wnd.content_view().set_layout(FillLayout::new(sb.view()));
        wnd.set_visibility(true);

        twm.step_unsend();

        let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
            .expect("could not get a single window");

        (sb, wnd, pal_hwnd)
    }

    #[test]
    fn knob_size_horizontal() {
        knob_size(false);
    }

    #[test]
    fn knob_size_vertical() {
        knob_size(true);
    }

    #[use_testing_wm(testing = "crate::testing")]
    fn knob_size(twm: &dyn TestingWm, vert: bool) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm, vert);
        let min_size = twm.wnd_attrs(&pal_hwnd).unwrap().min_size.t_if(vert);
        twm.step_unsend();
        twm.set_wnd_size(&pal_hwnd, [400, min_size[1]].t_if(vert));
        twm.step_unsend();

        let fr1 = sb.shared.frame.view().global_frame().t_if(vert);
        let fr2 = sb.shared.knob.view().global_frame().t_if(vert);

        assert!(fr2.size().x < fr1.size().x * 0.2);
        assert!(fr2.size().y > fr1.size().y * 0.1);
        assert!(fr1.contains_box(&fr2));
    }

    struct ValueUpdatingDragListener(Weak<Slider>, f64);

    impl ValueUpdatingDragListener {
        fn new(sb: &Rc<Slider>) -> Self {
            Self(Rc::downgrade(sb), sb.value())
        }
    }

    impl ScrollbarDragListener for ValueUpdatingDragListener {
        fn motion(&self, _: pal::Wm, new_value: f64) {
            if let Some(sb) = self.0.upgrade() {
                sb.set_value(new_value);
            }
        }
        fn cancel(&self, _: pal::Wm) {
            if let Some(sb) = self.0.upgrade() {
                sb.set_value(self.1);
            }
        }
    }

    #[test]
    fn knob_drag_horizontal() {
        knob_drag(false);
    }

    #[test]
    fn knob_drag_vertical() {
        knob_drag(true);
    }

    #[use_testing_wm(testing = "crate::testing")]
    fn knob_drag(twm: &dyn TestingWm, vert: bool) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm, vert);
        let min_size = twm.wnd_attrs(&pal_hwnd).unwrap().min_size.t_if(vert);
        twm.set_wnd_size(&pal_hwnd, [400, min_size[1]].t_if(vert));
        sb.set_value(0.0);
        sb.set_on_drag(enc!((sb) move |_| {
            ValueUpdatingDragListener::new(&sb).into()
        }));
        twm.step_unsend();

        let fr1 = sb.shared.frame.view().global_frame().t_if(vert);
        let fr2 = sb.shared.knob.view().global_frame().t_if(vert);

        debug!("fr1 = {:?}", fr1);
        debug!("fr2 = {:?}", fr2);

        let [st_x, y]: [f32; 2] = fr2.mid().into();
        let mut x = st_x;
        let mut value = sb.value();
        let drag = twm.raise_mouse_drag(&pal_hwnd, [x, y].t_if(vert).into(), 0);

        // Grab the knob
        drag.mouse_down([x, y].t_if(vert).into(), 0);

        assert!(sb.class_set().contains(ClassSet::ACTIVE));

        loop {
            x += 50.0;
            drag.mouse_motion([x, y].t_if(vert).into());
            twm.step_unsend();

            let new_value = sb.value();
            debug!("new_value = {}", new_value);
            assert!(new_value > value);
            assert!(new_value <= 1.0);

            value = new_value;

            if value >= 1.0 {
                break;
            }

            let fr2b = sb.shared.knob.view().global_frame().t_if(vert);
            debug!("fr2b = {:?}", fr2b);

            // The movement of the knob must follow the mouse pointer
            let offset = fr2b.min.x - fr2.min.x;
            assert_abs_diff_eq!(offset, x - st_x, epsilon = 0.1);

            // The length of the knob must not change
            assert_abs_diff_eq!(fr2b.size().x, fr2.size().x, epsilon = 0.1);

            assert!(
                x < 1000.0,
                "loop did not terminate within an expected duration"
            );
        }

        // Release the knob
        drag.mouse_up([x, y].t_if(vert).into(), 0);

        assert!(!sb.class_set().contains(ClassSet::ACTIVE));
    }

    #[test]
    fn trough_scroll_horizontal() {
        trough_scroll(false);
    }

    #[test]
    fn trough_scroll_vertical() {
        trough_scroll(true);
    }

    #[use_testing_wm(testing = "crate::testing")]
    fn trough_scroll(twm: &dyn TestingWm, vert: bool) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm, vert);
        let min_size = twm.wnd_attrs(&pal_hwnd).unwrap().min_size.t_if(vert);
        twm.set_wnd_size(&pal_hwnd, [400, min_size[1]].t_if(vert));
        sb.set_value(0.4);
        sb.set_on_drag(enc!((sb) move |_| {
            ValueUpdatingDragListener::new(&sb).into()
        }));
        twm.step_unsend();

        let fr1 = sb.shared.frame.view().global_frame().t_if(vert);
        let fr2 = sb.shared.knob.view().global_frame().t_if(vert);

        debug!("fr1 = {:?}", fr1);
        debug!("fr2 = {:?}", fr2);

        let y = fr2.mid().y;
        let value = sb.value();

        // Click the trough to set the value
        let x = fr1.min.x.average2(&fr2.min.x);
        info!("clicking at {:?}", [x, y]);
        let drag = twm.raise_mouse_drag(&pal_hwnd, [x, y].t_if(vert).into(), 0);
        drag.mouse_down([x, y].t_if(vert).into(), 0);
        twm.step_unsend();

        let new_value = sb.value();
        debug!("new_value = {}", new_value);
        assert!(new_value < value);

        drag.mouse_up([x, y].t_if(vert).into(), 0);
        twm.step_unsend();
    }

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn not_leaking_shared(twm: &dyn TestingWm) {
        let wm = twm.wm();

        let style_manager = Manager::global(wm);
        let sb = Rc::new(Slider::new(style_manager, false));

        // Store the drop detector in `Shared`
        let dropped = Rc::new(Cell::new(false));
        let drop_detector = OnDrop(Some(enc!((dropped) move || dropped.set(true))));
        sb.set_on_drag(move |_| {
            let _ = &drop_detector;
            unreachable!()
        });

        // Drop `Slider`
        drop(sb);
        twm.step_unsend();

        assert!(dropped.get(), "`Shared` was leaked");
    }

    struct OnDrop<F: FnOnce()>(Option<F>);

    impl<F: FnOnce()> Drop for OnDrop<F> {
        fn drop(&mut self) {
            (self.0.take().unwrap())();
        }
    }
}
