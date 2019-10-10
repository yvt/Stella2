//! Implements the scrollbar.
use alt_fp::FloatOrd;
use cggeom::prelude::*;
use cgmath::Point2;
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

use crate::{
    pal,
    ui::{
        layouts::FillLayout,
        theming::{
            ClassSet, ElemClassPath, Manager, ModifyArrangementArgs, PropKindFlags, Role,
            StyledBox, StyledBoxOverride,
        },
    },
    uicore::{HView, MouseDragListener, ViewFlags, ViewListener},
};

/// A scrollbar widget.
///
/// The widget is translucent and designed to be overlaid on contents.
#[derive(Debug)]
pub struct Scrollbar {
    shared: Rc<Shared>,
}

struct Shared {
    vertical: bool,
    value: Cell<f64>,
    page_step: Cell<f64>,
    on_drag: RefCell<Box<dyn Fn(pal::Wm) -> Box<dyn ScrollbarDragListener>>>,
    wrapper: HView,
    frame: StyledBox,
    thumb: StyledBox,
    layout_state: Cell<LayoutState>,
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Shared")
            .field("vertical", &self.vertical)
            .field("value", &self.value)
            .field("page_step", &self.page_step)
            .field("on_drag", &())
            .field("frame", &self.frame)
            .field("thumb", &self.thumb)
            .field("layout_state", &self.layout_state)
            .finish()
    }
}

/// Information obtained from the actual geometry of the scrollbar's elements.
#[derive(Copy, Clone, Debug, Default)]
struct LayoutState {
    thumb_start: f64,
    thumb_end: f64,
    clearance: f64,
}

/// Drag gesture handlers for [`Scrollbar`]. It has semantics similar to
/// `MouseDragListener`.
///
/// They are all called inside event handlers.
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

impl Scrollbar {
    pub fn new(style_manager: &'static Manager, vertical: bool) -> Self {
        let frame = StyledBox::new(style_manager, ViewFlags::default());
        frame.set_class_set(if vertical {
            ClassSet::SCROLLBAR | ClassSet::VERTICAL
        } else {
            ClassSet::SCROLLBAR
        });

        let thumb = StyledBox::new(style_manager, ViewFlags::default());
        thumb.set_parent_class_path(Some(frame.class_path().clone()));
        frame.set_subview(Role::Generic, Some(thumb.view().clone()));

        let wrapper = HView::new(ViewFlags::default() | ViewFlags::ACCEPT_MOUSE_DRAG);
        wrapper.set_layout(FillLayout::new(frame.view().clone()));

        let shared = Rc::new(Shared {
            vertical,
            value: Cell::new(0.0),
            page_step: Cell::new(0.1),
            on_drag: RefCell::new(Box::new(|_| Box::new(()))),
            wrapper,
            frame,
            thumb,
            layout_state: Cell::new(LayoutState::default()),
        });

        Shared::update_sb_override(&shared);

        shared.wrapper.set_listener(SbViewListener {
            shared: Rc::clone(&shared),
        });

        Self { shared }
    }

    /// Set the parent class path.
    pub fn set_parent_class_path(&mut self, parent_class_path: Option<Rc<ElemClassPath>>) {
        let (frame, thumb) = (&self.shared.frame, &self.shared.thumb);

        frame.set_parent_class_path(parent_class_path);
        thumb.set_parent_class_path(Some(frame.class_path().clone()));
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::SCROLLBAR`. Some bits (e.g., `ACTIVE`) are
    /// internally enforced and cannot be modified.
    pub fn set_class_set(&mut self, mut class_set: ClassSet) {
        let (frame, thumb) = (&self.shared.frame, &self.shared.thumb);

        // Protected bits
        class_set -= ClassSet::ACTIVE;
        class_set |= frame.class_set() & ClassSet::ACTIVE;
        frame.set_class_set(class_set);

        thumb.set_parent_class_path(Some(frame.class_path().clone()));
    }

    /// Get the class set of the inner `StyledBox`.
    pub fn class_set(&mut self) -> ClassSet {
        self.shared.frame.class_set()
    }

    /// Get the current value.
    pub fn value(&self) -> f64 {
        self.shared.value.get()
    }

    /// Set the current value in range `[0, 1]`.
    pub fn set_value(&mut self, new_value: f64) {
        debug_assert!(new_value >= 0.0, "{} >= 0.0", new_value);
        debug_assert!(new_value <= 1.0, "{} <= 1.0", new_value);

        self.shared.value.set(new_value);
        Shared::update_sb_override(&self.shared);
    }

    /// Get the page step size.
    pub fn page_step(&self) -> f64 {
        self.shared.page_step.get()
    }

    /// Set the page step size. Must be greater than or equal to zero. Can be
    /// infinity, in which case the scrollbar is disabled.
    pub fn set_page_step(&mut self, new_value: f64) {
        debug_assert!(new_value >= 0.0, "{} >= 0.0", new_value);

        self.shared.page_step.set(new_value);
        Shared::update_sb_override(&self.shared);
    }

    /// Set the factory function for gesture event handlers used when the user
    /// grabs the thumb.
    ///
    /// The function is called when the user starts a mouse drag gesture.
    pub fn set_on_drag(
        &mut self,
        handler: impl Fn(pal::Wm) -> Box<dyn ScrollbarDragListener> + 'static,
    ) {
        *self.shared.on_drag.borrow_mut() = Box::new(handler);
    }

    /// Get the view representing a styled box.
    pub fn view(&self) -> &HView {
        &self.shared.wrapper
    }
}

impl Shared {
    fn update_sb_override(this: &Rc<Shared>) {
        this.frame.set_override(SbStyledBoxOverride {
            value: this.value.get(),
            page_step: this.page_step.get(),
            shared: Rc::clone(this),
        })
    }

    fn set_active(&self, active: bool) {
        let (frame, thumb) = (&self.frame, &self.thumb);

        let mut class_set = frame.class_set();
        class_set.set(ClassSet::ACTIVE, active);
        frame.set_class_set(class_set);

        thumb.set_parent_class_path(Some(frame.class_path().clone()));
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
        // but it doesn't with floating-point arithmetic
        let thumb_ratio = if thumb_ratio_1.is_finite() {
            thumb_ratio_1
        } else {
            1.0
        };

        let min_thumb_len = size_traits.min[pri] as f64;
        let thumb_len = (bar_len * thumb_ratio).fmax(min_thumb_len);
        let clearance = bar_len - thumb_len;

        let thumb_start = bar_start + self.value * clearance;
        let thumb_end = thumb_start + thumb_len;
        frame.min[pri] = thumb_start as f32;
        frame.max[pri] = thumb_end as f32;

        // Layout feedback
        self.shared.layout_state.set(LayoutState {
            thumb_start,
            thumb_end,
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
        wm: pal::Wm,
        _: &HView,
        _loc: Point2<f32>,
        _button: u8,
    ) -> Box<dyn MouseDragListener> {
        Box::new(SbMouseDragListener {
            shared: Rc::clone(&self.shared),
            drag_start: Cell::new(None),
            listener: self.shared.on_drag.borrow()(wm),
        })
    }
}

/// Implements `MouseDragListener` for `Scrollbar`.
struct SbMouseDragListener {
    shared: Rc<Shared>,
    drag_start: Cell<Option<(f32, f64)>>,
    listener: Box<dyn ScrollbarDragListener>,
}

impl MouseDragListener for SbMouseDragListener {
    fn mouse_motion(&self, wm: pal::Wm, _: &HView, loc: Point2<f32>) {
        if let Some((init_pos, init_value)) = self.drag_start.get() {
            let pri = self.shared.vertical as usize;
            let clearance = self.shared.layout_state.get().clearance;

            if clearance == 0.0 {
                return;
            }

            let new_value = (init_value + (loc[pri] - init_pos) as f64 / clearance)
                .fmax(0.0)
                .fmin(1.0);

            self.listener.motion(wm, new_value);
        }
    }
    fn mouse_down(&self, wm: pal::Wm, _: &HView, loc: Point2<f32>, button: u8) {
        if button == 0 {
            // TODO: check clicking the trough

            let pri = self.shared.vertical as usize;
            self.drag_start
                .set(Some((loc[pri], self.shared.value.get())));

            self.shared.set_active(true);

            self.listener.down(wm, self.shared.value.get());
        }
    }
    fn mouse_up(&self, wm: pal::Wm, _: &HView, _loc: Point2<f32>, button: u8) {
        if button == 0 {
            if let Some(_) = self.drag_start.take() {
                self.shared.set_active(false);
                self.listener.up(wm);
            }
        }
    }
    fn cancel(&self, wm: pal::Wm, _: &HView) {
        if let Some(_) = self.drag_start.take() {
            self.shared.set_active(false);
        }
        self.listener.cancel(wm);
    }
}
