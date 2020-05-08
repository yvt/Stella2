//! Implements the scrollbar.
use alt_fp::FloatOrd;
use cggeom::prelude::*;
use cgmath::Point2;
use rc_borrow::RcBorrow;
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

use crate::{
    pal,
    prelude::*,
    ui::{
        layouts::FillLayout,
        theming::{
            roles, ClassSet, HElem, Manager, ModifyArrangementArgs, PropKindFlags, StyledBox,
            StyledBoxOverride, Widget,
        },
    },
    uicore::{HView, HViewRef, MouseDragListener, ViewFlags, ViewListener},
};

/// A scrollbar widget.
///
/// The widget is translucent and designed to be overlaid on contents.
#[derive(Debug)]
pub struct Scrollbar {
    shared: Rc<Shared>,
}

/// Specifies the direction of page step scrolling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    Incr = 1,
    Decr = -1,
}

struct Shared {
    vertical: bool,
    value: Cell<f64>,
    page_step: Cell<f64>,
    on_drag: RefCell<DragHandler>,
    on_page_step: RefCell<PageStepHandler>,
    wrapper: HView,
    frame: StyledBox,
    thumb: StyledBox,
    layout_state: Cell<LayoutState>,
}

type DragHandler = Box<dyn Fn(pal::Wm) -> Box<dyn ScrollbarDragListener>>;
type PageStepHandler = Box<dyn Fn(pal::Wm, Dir)>;

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("vertical", &self.vertical)
            .field("value", &self.value)
            .field("page_step", &self.page_step)
            .field("on_drag", &())
            .field("on_page_step", &())
            .field("frame", &self.frame)
            .field("thumb", &self.thumb)
            .field("layout_state", &self.layout_state)
            .finish()
    }
}

/// Information obtained from the actual geometry of the scrollbar's elements.
#[derive(Copy, Clone, Debug, Default)]
struct LayoutState {
    thumb_start: f32,
    thumb_end: f32,
    clearance: f64,
}

/// Drag gesture handlers for [`Scrollbar`]. It has semantics similar to
/// `MouseDragListener`.
///
/// They are all called inside `invoke_on_update`. The event rate is limited by
/// the screen update rate.
pub trait ScrollbarDragListener {
    /// The thumb is about to be moved. `new_value` specifies the current
    /// [`value`].
    ///
    /// [`value`]: crate::ui::views::scrollbar::Scrollbar::value
    fn down(&self, _: pal::Wm, _new_value: f64) {}

    /// The thumb is being moved. `new_value` specifies the new [`value`].
    /// The implementation is responsible for updating `Scrollbar` with a new
    /// value.
    ///
    /// [`value`]: crate::ui::views::scrollbar::Scrollbar::value
    fn motion(&self, _: pal::Wm, _new_value: f64) {}

    /// The thumb was moved.
    fn up(&self, _: pal::Wm) {}

    /// The drag gesture was cancelled. The implementation is responsible for
    /// updating `Scrollbar` with an original value.
    fn cancel(&self, _: pal::Wm) {}
}

impl ScrollbarDragListener for () {}

impl<T: ScrollbarDragListener + 'static> From<T> for Box<dyn ScrollbarDragListener> {
    fn from(x: T) -> Box<dyn ScrollbarDragListener> {
        Box::new(x)
    }
}

impl Scrollbar {
    pub fn new(style_manager: &'static Manager, vertical: bool) -> Self {
        let frame = StyledBox::new(style_manager, ViewFlags::ACCEPT_MOUSE_OVER);
        frame.set_class_set(if vertical {
            ClassSet::SCROLLBAR | ClassSet::VERTICAL
        } else {
            ClassSet::SCROLLBAR
        });
        frame.set_auto_class_set(ClassSet::HOVER | ClassSet::FOCUS);

        let thumb = StyledBox::new(style_manager, ViewFlags::default());
        frame.set_child(roles::GENERIC, Some(&thumb));

        let wrapper = HView::new(ViewFlags::ACCEPT_MOUSE_DRAG | ViewFlags::NO_FOCUS_ON_CLICK);
        wrapper.set_layout(FillLayout::new(frame.view()));

        let shared = Rc::new(Shared {
            vertical,
            value: Cell::new(0.0),
            page_step: Cell::new(0.1),
            on_drag: RefCell::new(Box::new(|_| Box::new(()))),
            on_page_step: RefCell::new(Box::new(|_, _| {})),
            wrapper,
            frame,
            thumb,
            layout_state: Cell::new(LayoutState::default()),
        });

        Shared::update_sb_override((&shared).into());

        shared.wrapper.set_listener(SbViewListener {
            shared: Rc::clone(&shared),
        });

        Self { shared }
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::SCROLLBAR`. Some bits (e.g., `ACTIVE`) are
    /// internally enforced and cannot be modified.
    pub fn set_class_set(&self, mut class_set: ClassSet) {
        let frame = &self.shared.frame;

        // Protected bits
        let protected = ClassSet::ACTIVE | ClassSet::HOVER;
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
        Shared::update_sb_override((&self.shared).into());
    }

    /// Get the page step size.
    pub fn page_step(&self) -> f64 {
        self.shared.page_step.get()
    }

    /// Set the page step size. Must be greater than or equal to zero. Can be
    /// infinity, in which case the scrollbar is disabled.
    pub fn set_page_step(&self, new_value: f64) {
        debug_assert!(new_value >= 0.0, "{} >= 0.0", new_value);

        self.shared.page_step.set(new_value);
        Shared::update_sb_override((&self.shared).into());
    }

    /// Set the factory function for gesture event handlers used when the user
    /// grabs the thumb.
    ///
    /// The function is called when the user starts a mouse drag gesture.
    pub fn set_on_drag(
        &self,
        handler: impl Fn(pal::Wm) -> Box<dyn ScrollbarDragListener> + 'static,
    ) {
        *self.shared.on_drag.borrow_mut() = Box::new(handler);
    }

    /// Set the handler function called when the user clicks the trough (the
    /// region outside the thumb).
    ///
    /// The function is called through `invoke_on_update`.
    pub fn set_on_page_step(&self, handler: impl Fn(pal::Wm, Dir) + 'static) {
        *self.shared.on_page_step.borrow_mut() = Box::new(handler);
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

impl Widget for Scrollbar {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

impl Shared {
    fn update_sb_override(this: RcBorrow<'_, Shared>) {
        this.frame.set_override(SbStyledBoxOverride {
            value: this.value.get(),
            page_step: this.page_step.get(),
            shared: RcBorrow::upgrade(this),
        })
    }

    fn set_active(&self, active: bool) {
        let frame = &self.frame;

        let mut class_set = frame.class_set();
        class_set.set(ClassSet::ACTIVE, active);
        frame.set_class_set(class_set);
    }
}

/// Implements `StyledBoxOverride` for `Scrollbar`.
struct SbStyledBoxOverride {
    value: f64,
    page_step: f64,
    /// This reference to `Shared` is used to provide layout feedback. The above
    /// fields should remain to ensure the logical immutability of this
    /// `StyledBoxOverride`. (This is actually never a problem in the current
    /// implementation of `StyledBox`, though.)
    shared: Rc<Shared>,
}

impl StyledBoxOverride for SbStyledBoxOverride {
    fn modify_arrangement(
        &self,
        ModifyArrangementArgs {
            size_traits, frame, ..
        }: ModifyArrangementArgs<'_>,
    ) {
        let pri = self.shared.vertical as usize;

        let bar_len = frame.size()[pri] as f64;
        let bar_start = frame.min[pri] as f64;

        // A scrollbar represents the entire length (`1 + page_step`) of
        // a scrollable area, and the thumb the visible portion of size
        // `page_step`. Calculate the size of the thumb based on this parallel.
        let thumb_ratio_1 = self.page_step / (1.0 + self.page_step);
        // `thumb_ratio_1` asymptotically approaches `1` as `thumb_ratio_1` → ∞
        // but it might generate NaN with floating-point arithmetic. (And even
        // 0 with a non-default rounding mode, which we assume never happens)
        // `x.fmin(1.0)` returns `1.0` if `x` is NaN. Bonus: `fmin` lowers to
        // `minsd` in x86_64
        let thumb_ratio = thumb_ratio_1.fmin(1.0);

        let min_thumb_len = size_traits.min[pri] as f64;
        let thumb_len = (bar_len * thumb_ratio).fmax(min_thumb_len);
        let clearance = bar_len - thumb_len;

        let thumb_start = bar_start + self.value * clearance;
        let thumb_end = thumb_start + thumb_len;
        frame.min[pri] = thumb_start as f32;
        frame.max[pri] = thumb_end as f32;

        // Layout feedback
        self.shared.layout_state.set(LayoutState {
            thumb_start: thumb_start as f32,
            thumb_end: thumb_end as f32,
            clearance,
        });
    }

    fn dirty_flags(&self, other: &dyn StyledBoxOverride) -> PropKindFlags {
        use as_any::Downcast;
        if let Some(other) = (*other).downcast_ref::<Self>() {
            if (self.value, self.page_step) == (other.value, other.page_step) {
                PropKindFlags::empty()
            } else {
                PropKindFlags::LAYOUT
            }
        } else {
            PropKindFlags::all()
        }
    }
}

/// Implements `ViewListener` for `Scrollbar`.
struct SbViewListener {
    shared: Rc<Shared>,
}

impl ViewListener for SbViewListener {
    fn mouse_drag(
        &self,
        _: pal::Wm,
        _: HViewRef<'_>,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn MouseDragListener> {
        Box::new(SbMouseDragListener {
            shared: Rc::clone(&self.shared),
            drag_start: Cell::new(None),
            listener: RefCell::new(None),
        })
    }
}

/// Implements `MouseDragListener` for `Scrollbar`.
struct SbMouseDragListener {
    shared: Rc<Shared>,
    drag_start: Cell<Option<(f32, f64)>>,
    listener: RefCell<Option<Box<dyn ScrollbarDragListener>>>,
}

impl MouseDragListener for SbMouseDragListener {
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

            let page_step_dir = if local_loc > layout_state.thumb_end {
                Some(Dir::Incr)
            } else if local_loc < layout_state.thumb_start {
                Some(Dir::Decr)
            } else {
                None
            };

            if let Some(dir) = page_step_dir {
                // Trough clicked
                // TODO: Support cancellation?
                let shared = Rc::clone(&self.shared);
                wm.invoke_on_update(move |wm| {
                    shared.on_page_step.borrow()(wm, dir);
                });
            } else {
                // Dragging the thumb
                self.drag_start.set(Some((loc, self.shared.value.get())));
                self.shared.set_active(true);

                if self.listener.borrow().is_none() {
                    // TODO: Refactor - extra `Box`, inconsistent uses of `invoke_on_update`
                    let listener = self.shared.on_drag.borrow()(wm);
                    let listener = Box::new(ListenerOnUpdateFilter::new(listener));
                    *self.listener.borrow_mut() = Some(listener);
                }

                (self.listener.borrow().as_ref().unwrap()).down(wm, self.shared.value.get());
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

/// Wraps `ScrollbarDragListener` to limit the event generation rate using
/// `invoke_on_update`.
struct ListenerOnUpdateFilter {
    inner: Rc<ListenerOnUpdateFilterInner>,
    motion_queued: Cell<bool>,
}

struct ListenerOnUpdateFilterInner {
    listener: Box<dyn ScrollbarDragListener>,
    motion_value: Cell<f64>,
}

impl ListenerOnUpdateFilter {
    fn new(listener: Box<dyn ScrollbarDragListener>) -> Self {
        Self {
            inner: Rc::new(ListenerOnUpdateFilterInner {
                listener,
                motion_value: Cell::new(0.0),
            }),
            motion_queued: Cell::new(false),
        }
    }
}

impl ScrollbarDragListener for ListenerOnUpdateFilter {
    fn down(&self, wm: pal::Wm, new_value: f64) {
        self.motion_queued.set(false);

        let inner = Rc::clone(&self.inner);
        wm.invoke_on_update(move |wm| {
            inner.listener.down(wm, new_value);
        });
    }

    fn motion(&self, wm: pal::Wm, new_value: f64) {
        // Only store the latest value
        self.inner.motion_value.set(new_value);

        // Do not enqueue more than one `motion` event per frame
        if self.motion_queued.get() {
            return;
        }

        let inner = Rc::clone(&self.inner);
        wm.invoke_on_update(move |wm| {
            inner.listener.motion(wm, inner.motion_value.get());
        });
    }

    fn up(&self, wm: pal::Wm) {
        let inner = Rc::clone(&self.inner);
        wm.invoke_on_update(move |wm| {
            inner.listener.up(wm);
        });
    }

    fn cancel(&self, wm: pal::Wm) {
        let inner = Rc::clone(&self.inner);
        wm.invoke_on_update(move |wm| {
            inner.listener.cancel(wm);
        });
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

    fn make_wnd(twm: &dyn TestingWm, vertical: bool) -> (Rc<Scrollbar>, HWnd, pal::HWnd) {
        let wm = twm.wm();

        let style_manager = Manager::global(wm);
        let sb = Rc::new(Scrollbar::new(style_manager, vertical));

        let wnd = HWnd::new(wm);
        wnd.content_view().set_layout(FillLayout::new(sb.view()));
        wnd.set_visibility(true);

        twm.step_unsend();

        let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
            .expect("could not get a single window");

        (sb, wnd, pal_hwnd)
    }

    #[test]
    fn thumb_size_horizontal() {
        thumb_size(false);
    }

    #[test]
    fn thumb_size_vertical() {
        thumb_size(true);
    }

    #[use_testing_wm(testing = "crate::testing")]
    fn thumb_size(twm: &dyn TestingWm, vert: bool) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm, vert);
        let min_size = twm.wnd_attrs(&pal_hwnd).unwrap().min_size.t_if(vert);
        sb.set_page_step(0.02);
        twm.step_unsend();
        twm.set_wnd_size(&pal_hwnd, [400, min_size[1]].t_if(vert));
        twm.step_unsend();

        let fr1 = sb.shared.frame.view().global_frame().t_if(vert);
        let fr2 = sb.shared.thumb.view().global_frame().t_if(vert);

        assert!(fr2.size().x < fr1.size().x * 0.2);
        assert!(fr2.size().y > fr1.size().y * 0.1);
        assert!(fr1.contains_box(&fr2));
    }

    struct ValueUpdatingDragListener(Weak<Scrollbar>, f64);

    impl ValueUpdatingDragListener {
        fn new(sb: &Rc<Scrollbar>) -> Self {
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
    fn thumb_drag_horizontal() {
        thumb_drag(false);
    }

    #[test]
    fn thumb_drag_vertical() {
        thumb_drag(true);
    }

    #[use_testing_wm(testing = "crate::testing")]
    fn thumb_drag(twm: &dyn TestingWm, vert: bool) {
        let (sb, _hwnd, pal_hwnd) = make_wnd(twm, vert);
        let min_size = twm.wnd_attrs(&pal_hwnd).unwrap().min_size.t_if(vert);
        twm.set_wnd_size(&pal_hwnd, [400, min_size[1]].t_if(vert));
        sb.set_page_step(0.1);
        sb.set_value(0.0);
        sb.set_on_drag(enc!((sb) move |_| {
            ValueUpdatingDragListener::new(&sb).into()
        }));
        sb.set_on_page_step(|_, _| unreachable!());
        twm.step_unsend();

        let fr1 = sb.shared.frame.view().global_frame().t_if(vert);
        let fr2 = sb.shared.thumb.view().global_frame().t_if(vert);

        debug!("fr1 = {:?}", fr1);
        debug!("fr2 = {:?}", fr2);

        let [st_x, y]: [f32; 2] = fr2.mid().into();
        let mut x = st_x;
        let mut value = sb.value();
        let drag = twm.raise_mouse_drag(&pal_hwnd, [x, y].t_if(vert).into(), 0);

        // Grab the thumb
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

            let fr2b = sb.shared.thumb.view().global_frame().t_if(vert);
            debug!("fr2b = {:?}", fr2b);

            // The movement of the thumb must follow the mouse pointer
            let offset = fr2b.min.x - fr2.min.x;
            assert_abs_diff_eq!(offset, x - st_x, epsilon = 0.1);

            // The length of the thumb must not change
            assert_abs_diff_eq!(fr2b.size().x, fr2.size().x, epsilon = 0.1);

            assert!(
                x < 1000.0,
                "loop did not terminate within an expected duration"
            );
        }

        // Release the thumb
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
        sb.set_page_step(0.1);
        sb.set_value(0.4);
        sb.set_on_drag(|_| unreachable!());
        sb.set_on_page_step(enc!((sb) move |_, dir| {
            debug!("on_page_step({:?})", dir);
            let new_value = sb.value() + sb.page_step() * dir as i8 as f64;
            sb.set_value(new_value.fmax(0.0).fmin(1.0));
        }));
        twm.step_unsend();

        let fr1 = sb.shared.frame.view().global_frame().t_if(vert);
        let fr2 = sb.shared.thumb.view().global_frame().t_if(vert);

        debug!("fr1 = {:?}", fr1);
        debug!("fr2 = {:?}", fr2);

        let y = fr2.mid().y;
        let mut value = sb.value();

        // Click the trough to lower the value
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
        drop(drag);

        value = new_value;

        // Click the trough to raise the value
        let x = fr1.max.x.average2(&fr2.max.x);
        info!("clicking at {:?}", [x, y]);
        let drag = twm.raise_mouse_drag(&pal_hwnd, [x, y].t_if(vert).into(), 0);
        drag.mouse_down([x, y].t_if(vert).into(), 0);
        twm.step_unsend();

        let new_value = sb.value();
        debug!("new_value = {}", new_value);
        assert!(new_value > value);

        drag.mouse_up([x, y].t_if(vert).into(), 0);
        twm.step_unsend();
    }
}
