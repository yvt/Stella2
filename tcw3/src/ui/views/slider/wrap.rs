//! Implements a high-level interface for `SliderRaw`.
use alt_fp::FloatOrd;
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
};
use subscriber_list::SubscriberList;

use super::{Dir, ScrollbarDragListener};
use crate::{
    pal,
    ui::theming::{self, ClassSet, HElem, Role, Widget},
    uicore::{HView, HViewRef, Sub},
    utils::resetiter,
};

// ---------------------------------------------------------------------------

/// Specifies the behavior of [`Slider`] in certain aspects such as a permitted
/// set of values and a response to arrow keys.
pub trait SliderTraits {
    /// Round the specified raw value to the nearest permitted value.
    fn filter_value(&self, value: f64) -> f64 {
        value
    }

    /// Adjust the specified value by one step and return it.
    fn step(&self, value: f64, dir: Dir) -> f64;
}

impl<T: SliderTraits + 'static> From<T> for Box<dyn SliderTraits> {
    fn from(x: T) -> Box<dyn SliderTraits> {
        Box::new(x)
    }
}

/// An implementation of `SliderTraits` that doesn't constrain the value.
#[derive(Default, Debug, Clone, Copy)]
pub struct SmoothSliderTraits {}

impl SmoothSliderTraits {
    /// Construct a `SmoothSliderTraits`.
    pub fn new() -> Self {
        Self {}
    }
}

impl SliderTraits for SmoothSliderTraits {
    fn step(&self, value: f64, dir: Dir) -> f64 {
        (value + dir as i8 as f64 * 0.05).fmax(0.0).fmin(1.0)
    }
}

/// An implementation of `SliderTraits` that makes the value “snap” to
/// uniformly arranged reference points.
#[derive(Debug, Clone, Copy)]
pub struct UniformStepSliderTraits {
    num_segments: f64,
}

impl UniformStepSliderTraits {
    /// Construct a `UniformStepSliderTraits`.
    pub fn new(num_segments: u32) -> Self {
        Self {
            num_segments: num_segments as f64,
        }
    }
}

impl SliderTraits for UniformStepSliderTraits {
    fn filter_value(&self, value: f64) -> f64 {
        (value * self.num_segments).round() / self.num_segments
    }

    fn step(&self, value: f64, dir: Dir) -> f64 {
        (value + dir as i8 as f64 / self.num_segments)
            .fmax(0.0)
            .fmin(1.0)
    }
}

// ---------------------------------------------------------------------------

/// A high-level interface for [`SliderRaw`].
///
/// [`SliderRaw`]: super::SliderRaw
///
/// `Slider` automatically updates the current value in response to a user's
/// actions and raises events ([`changed`], [`changing`]) to notify the
/// application. Compared to `SliderRaw`, `Slider`'s programming interface is
/// more concerned about changes in the value rather than handling user inputs.
///
/// [`changed`]: Slider::subscribe_changed
/// [`changing`]: Slider::subscribe_changing
///
/// `Slider` maintains two values: `value` and `uncommitted_value`. Most
/// operations change both of these, while long-taking operations with a
/// possibility of cancellation such as dragging the knob only updates
/// `uncommitted_value` in real-time before finally “committing” the final value
/// by updating `value`.
///
/// Certain aspects of `Slider`'s behavior are specified by an associated
/// [`SliderTraits`]. The default value [`SmoothSliderTraits`]`::new()` allows
/// it to take any fractional value, whereas [`UniformStepSliderTraits`]
/// restricts the movement to uniformly arranged points. `SliderTraits` can be
/// assigned to a `Slider` by [`Slider::set_traits`].
#[derive(Debug)]
pub struct Slider {
    shared: Rc<Shared>,
}

struct Shared {
    wm: pal::Wm,
    slider_raw: super::SliderRaw,

    traits: RefCell<Box<dyn SliderTraits>>,

    changed_handlers: RefCell<SubscriberList<Box<dyn Fn(pal::Wm)>>>,
    changing_handlers: RefCell<SubscriberList<Box<dyn Fn(pal::Wm)>>>,

    /// The committed value.
    ///
    /// When there are no instances of `DragListener` having the control of
    /// `slider_raw`, this is equal to `slider_raw.value()` (the uncommitted
    /// value).
    value: Cell<f64>,

    /// Indicates which instance of `DragListener` has ownership of the value.
    /// To cause the current `DragListener` to relinquish the control, just
    /// increment this value.
    drag_ticket: Cell<usize>,
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("wm", &self.wm)
            .field("slider_raw", &self.slider_raw)
            .field("value", &self.value)
            .field("drag_ticket", &self.drag_ticket)
            .finish()
    }
}

impl Slider {
    /// Construct a `Slider`.
    pub fn new(wm: pal::Wm, style_manager: &'static theming::Manager, vertical: bool) -> Self {
        let slider_raw = super::SliderRaw::new(style_manager, vertical);

        let shared = Rc::new(Shared {
            wm,
            slider_raw,
            traits: RefCell::new(Box::new(SmoothSliderTraits::new())),
            changed_handlers: RefCell::new(SubscriberList::new()),
            changing_handlers: RefCell::new(SubscriberList::new()),
            value: Cell::new(0.0),
            drag_ticket: Cell::new(0),
        });

        let shared_weak = Rc::downgrade(&shared);
        shared.slider_raw.set_on_drag(move |_| {
            if let Some(shared) = shared_weak.upgrade() {
                shared.handle_on_drag()
            } else {
                Box::new(())
            }
        });

        let shared_weak = Rc::downgrade(&shared);
        shared.slider_raw.set_on_step(move |_, dir| {
            if let Some(shared) = shared_weak.upgrade() {
                shared.handle_on_step(dir);
            }
        });

        Self { shared }
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::SLIDER`. Some bits (e.g., `ACTIVE`) are
    /// internally enforced and cannot be modified.
    pub fn set_class_set(&self, class_set: ClassSet) {
        self.shared.slider_raw.set_class_set(class_set);
    }

    /// Get the class set of the inner `StyledBox`.
    pub fn class_set(&self) -> ClassSet {
        self.shared.slider_raw.class_set()
    }

    /// Assign the specified [`SliderTraits`].
    ///
    /// The default value is [`SmoothSliderTraits`]`::new()`.
    #[momo::momo]
    pub fn set_traits(&self, t: impl Into<Box<dyn SliderTraits>>) {
        *self.shared.traits.borrow_mut() = t.into();
        self.set_value(self.value());
    }

    /// Get the current value.
    ///
    /// This value doesn't update during a mouse drag operation. Use
    /// [`uncommitted_value`] to get a real-time value.
    ///
    /// [`uncommitted_value`]: Slider::uncommitted_value
    pub fn value(&self) -> f64 {
        self.shared.value.get()
    }

    /// Set the current value in range `[0, 1]`.
    ///
    /// Changing the value cancels any ongoing mouse drag operation.
    pub fn set_value(&self, new_value: f64) {
        self.shared.set_value(new_value);
    }

    /// Get the current uncommitted value.
    pub fn uncommitted_value(&self) -> f64 {
        self.shared.slider_raw.value()
    }

    /// Add a function to be called whenever `value` changes.
    pub fn subscribe_changed(&self, cb: Box<dyn Fn(pal::Wm)>) -> Sub {
        self.shared
            .changed_handlers
            .borrow_mut()
            .insert(cb)
            .untype()
    }

    /// Add a function to be called whenever `uncommitted_value` changes.
    pub fn subscribe_changing(&self, cb: Box<dyn Fn(pal::Wm)>) -> Sub {
        self.shared
            .changing_handlers
            .borrow_mut()
            .insert(cb)
            .untype()
    }

    /// Set the tick mark positions.
    pub fn set_ticks<I>(&self, new_ticks: I)
    where
        I: resetiter::IntoResetIter<Item = f64>,
        I::IntoResetIter: 'static,
    {
        self.shared.slider_raw.set_ticks(new_ticks);
    }

    /// Arrange tick marks uniformly by calling `set_ticks`.
    ///
    /// `num_segments` must not be zero.
    pub fn set_uniform_ticks(&self, num_segments: usize) {
        self.shared.slider_raw.set_uniform_ticks(num_segments);
    }

    /// Set custom label views attached to specified values.
    pub fn set_labels<'a>(&self, children: impl AsRef<[(Role, Option<(f64, &'a dyn Widget)>)]>) {
        self.shared.slider_raw.set_labels(children);
    }

    /// Get an owned handle to the view representing the widget.
    pub fn view(&self) -> HView {
        self.shared.slider_raw.view()
    }

    /// Borrow the handle to the view representing the widget.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.shared.slider_raw.view_ref()
    }

    /// Get the styling element representing the widget.
    pub fn style_elem(&self) -> HElem {
        self.shared.slider_raw.style_elem()
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
    fn handle_on_drag(self: Rc<Shared>) -> Box<dyn ScrollbarDragListener> {
        // Cancel any active association with `DragListener`
        self.drag_ticket.set(self.drag_ticket.get() + 1);

        // Construct a brand new `DragListener` and return it
        Box::new(DragListener {
            shared: Rc::downgrade(&self),
            drag_ticket: self.drag_ticket.get(),
        })
    }

    fn handle_on_step(self: Rc<Shared>, dir: Dir) {
        let new_value = {
            let traits = self.traits.borrow();
            traits.filter_value(traits.step(self.slider_raw.value(), dir))
        };
        self.set_value(new_value);
    }

    fn set_value(&self, value: f64) {
        let value = self.traits.borrow().filter_value(value);

        if value == self.value.get() {
            return;
        }

        // Cancel any active association with `DragListener`
        self.drag_ticket.set(self.drag_ticket.get() + 1);

        self.slider_raw.set_value(value);
        self.value.set(value);

        self.raise_changing();
        self.raise_changed();
    }

    fn raise_changed(&self) {
        let handlers = self.changed_handlers.borrow();
        for handler in handlers.iter() {
            handler(self.wm);
        }
    }
    fn raise_changing(&self) {
        let handlers = self.changing_handlers.borrow();
        for handler in handlers.iter() {
            handler(self.wm);
        }
    }
}

struct DragListener {
    shared: Weak<Shared>,
    /// If this value is equal to `shared.drag_ticket`, `DragListener` can
    /// control `shared`.
    drag_ticket: usize,
}

impl DragListener {
    fn get_shared_checking_ticket(&self) -> Option<Rc<Shared>> {
        self.shared
            .upgrade()
            .filter(|s| s.drag_ticket.get() == self.drag_ticket)
    }
}

impl ScrollbarDragListener for DragListener {
    fn down(&self, _: pal::Wm, _new_value: f64) {}

    fn motion(&self, _: pal::Wm, new_value: f64) {
        // Update the uncommitted value (uncommitted_value ← new_value)
        if let Some(shared) = self.get_shared_checking_ticket() {
            shared
                .slider_raw
                .set_value(shared.traits.borrow().filter_value(new_value));
            shared.raise_changing();
        }
    }

    fn up(&self, _: pal::Wm) {
        // Commit the new value (value ← uncommitted_value)
        if let Some(shared) = self.get_shared_checking_ticket() {
            shared.value.set(shared.slider_raw.value());
            shared.raise_changed();
        }
    }

    fn cancel(&self, _: pal::Wm) {
        // Revert to the committed (original) value (uncomitted_value ← value)
        if let Some(shared) = self.get_shared_checking_ticket() {
            shared.slider_raw.set_value(shared.value.get());
            shared.raise_changing();
        }
    }
}

#[cfg(test)]
mod tests {
    use cggeom::prelude::*;
    use enclose::enc;
    use try_match::try_match;

    use super::*;
    use crate::{
        pal,
        testing::{prelude::*, use_testing_wm},
        ui::{layouts::FillLayout, theming::Manager},
        uicore::HWnd,
    };

    fn make_wnd(twm: &dyn TestingWm) -> (Rc<Slider>, HWnd, pal::HWnd) {
        let wm = twm.wm();

        let style_manager = Manager::global(wm);
        let sb = Rc::new(Slider::new(wm, style_manager, false /* horizontal */));

        let wnd = HWnd::new(wm);
        wnd.content_view().set_layout(FillLayout::new(sb.view()));
        wnd.set_visibility(true);

        twm.step_unsend();

        let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
            .expect("could not get a single window");

        let min_size = twm.wnd_attrs(&pal_hwnd).unwrap().min_size;
        twm.set_wnd_size(&pal_hwnd, [400, min_size[1]]);
        twm.step_unsend();

        (sb, wnd, pal_hwnd)
    }

    #[derive(Debug, PartialEq)]
    enum Event {
        Changing(St),
        Changed(St),
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct St {
        value: f64,
        uncommitted_value: f64,
    }

    impl St {
        fn capture(s: &Slider) -> Self {
            Self {
                value: s.value(),
                uncommitted_value: s.uncommitted_value(),
            }
        }
    }

    fn setup_event_collector(slider: &Rc<Slider>) -> Rc<RefCell<Vec<Event>>> {
        let slider_weak = Rc::downgrade(slider);
        let events = Rc::new(RefCell::new(Vec::new()));
        slider.subscribe_changing(Box::new(enc!((events, slider_weak) move |_| {
            let slider = slider_weak.upgrade().unwrap();
            let e = Event::Changing(St::capture(&slider));
            log::trace!("{:?}", e);
            events.borrow_mut().push(e);
        })));
        slider.subscribe_changed(Box::new(enc!((events, slider_weak) move |_| {
            let slider = slider_weak.upgrade().unwrap();
            let e = Event::Changed(St::capture(&slider));
            log::trace!("{:?}", e);
            events.borrow_mut().push(e);
        })));
        events
    }

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn knob_drag(twm: &dyn TestingWm) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm);
        let events_cell = setup_event_collector(&sb);

        let fr1 = sb.shared.slider_raw.shared.frame.view().global_frame();
        let fr2 = sb.shared.slider_raw.shared.knob.view().global_frame();

        log::debug!("fr1 = {:?}", fr1);
        log::debug!("fr2 = {:?}", fr2);

        let [x, y]: [f32; 2] = fr2.mid().into();

        // Grab the knob
        let drag = twm.raise_mouse_drag(&pal_hwnd, [x, y].into(), 0);
        drag.mouse_down([x, y].into(), 0);
        twm.step_unsend();

        // Move it
        drag.mouse_motion([x + 50.0, y].into());
        twm.step_unsend();

        {
            let events = events_cell.replace(Vec::new());
            log::trace!("events = {:?}", events);
            assert!(
                events.iter().all(|e| matches!(e, Event::Changing(_))),
                "expected only `changing` events, but got: {:?}",
                events
            );
        }

        // Release it
        drag.mouse_up([x + 50.0, y].into(), 0);
        twm.step_unsend();

        {
            let events = events_cell.replace(Vec::new());
            log::trace!("events = {:?}", events);
            assert!(
                events.iter().any(|e| matches!(e, Event::Changed(_))),
                "expected any `changed` events, but got: {:?}",
                events
            );
        }
    }

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn cancel_drag_by_set_value(twm: &dyn TestingWm) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm);
        let events_cell = setup_event_collector(&sb);

        let fr1 = sb.shared.slider_raw.shared.frame.view().global_frame();
        let fr2 = sb.shared.slider_raw.shared.knob.view().global_frame();

        log::debug!("fr1 = {:?}", fr1);
        log::debug!("fr2 = {:?}", fr2);

        let [x, y]: [f32; 2] = fr2.mid().into();

        // Grab the knob
        let drag = twm.raise_mouse_drag(&pal_hwnd, [x, y].into(), 0);
        drag.mouse_down([x, y].into(), 0);
        twm.step_unsend();

        // Move it
        drag.mouse_motion([x + 50.0, y].into());
        twm.step_unsend();

        {
            let events = events_cell.replace(Vec::new());
            log::trace!("events = {:?}", events);
        }

        // Call `set_value`, which will cancel the drag
        sb.set_value(0.5);
        {
            let events = events_cell.replace(Vec::new());
            log::trace!("events = {:?}", events);
            let expected_st = St {
                value: 0.5,
                uncommitted_value: 0.5,
            };
            assert!(
                events.iter().any(|e| *e == Event::Changed(expected_st))
                    && events.iter().any(|e| *e == Event::Changing(expected_st)),
                "expected `changed` and `changing` events with correct states, but got: {:?}",
                events
            );
        }

        // Moving it further has no effect
        drag.mouse_motion([x + 100.0, y].into());
        twm.step_unsend();

        {
            let events = events_cell.replace(Vec::new());
            assert!(
                events.is_empty(),
                "expected no events, but got: {:?}",
                events
            );
        }
    }
}
