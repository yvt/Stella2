use cgmath::Point2;
use flags_macro::flags;
use owning_ref::OwningRef;
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

use super::{
    scrollbar::{
        table_edit_to_scrollbar_page_step, table_edit_to_scrollbar_value,
        TableScrollbarDragListener,
    },
    scrollwheel::TableScrollModel,
    LineTy, Table, TableFlags,
};
use crate::{
    pal,
    prelude::*,
    ui::{
        layouts::FillLayout,
        mixins::scrollwheel::{ScrollAxisFlags, ScrollModel, ScrollWheelMixin},
        theming::{ClassSet, HElem, Manager, Role, StyledBox, Widget},
        views::Scrollbar,
    },
    uicore::{HView, HViewRef, ScrollDelta, ScrollListener, SizeTraits, ViewFlags, ViewListener},
};

/// Wraps [`Table`] to support scrolling.
#[derive(Debug)]
pub struct ScrollableTable {
    inner: Rc<Inner>,
}

#[derive(Debug)]
struct Inner {
    wrapper: HView,
    styled_box: StyledBox,
    table: Table,
    scrollbars: [Scrollbar; 2],
    drag_active: [Cell<bool>; 2],
    scroll_mixin: ScrollWheelMixin,
}

impl ScrollableTable {
    /// Construct a `ScrollableTable`.
    pub fn new(style_manager: &'static Manager) -> Self {
        let styled_box = StyledBox::new(style_manager, ViewFlags::default());
        let table = Table::new();
        let scrollbars = [
            Scrollbar::new(style_manager, false),
            Scrollbar::new(style_manager, true),
        ];

        styled_box.set_subview(Role::Generic, Some(table.view()));
        styled_box.set_child(Role::HorizontalScrollbar, Some(&scrollbars[0]));
        styled_box.set_child(Role::VerticalScrollbar, Some(&scrollbars[1]));

        styled_box.set_class_set(ClassSet::SCROLL_CONTAINER);

        // Create a view for receiving scroll wheel events
        let wrapper = HView::new(ViewFlags::default() | ViewFlags::ACCEPT_SCROLL);
        wrapper.set_layout(FillLayout::new(styled_box.view()));

        let this = Self {
            inner: Rc::new(Inner {
                wrapper,
                styled_box,
                table,
                scrollbars,
                drag_active: [Cell::new(false), Cell::new(false)],
                scroll_mixin: ScrollWheelMixin::new(),
            }),
        };

        this.inner.update_class_set();
        this.inner.update_scrollbar_value();

        // Register event handlers
        let inner_weak = Rc::downgrade(&this.inner);
        this.inner.table.subscribe_model_update(Box::new(move || {
            if let Some(inner) = inner_weak.upgrade() {
                // The handler may be called from `Layout`, where most actions
                // are restricted
                pal::Wm::global().invoke_on_update(move |_| {
                    inner.update_class_set();
                    inner.update_scrollbar_value();
                });
            }
        }));

        for &line_ty in &[LineTy::Col, LineTy::Row] {
            let inner_weak = Rc::downgrade(&this.inner);
            this.inner.scrollbars[line_ty.i()].set_on_drag(Box::new(move |_| {
                if let Some(inner) = inner_weak.upgrade() {
                    struct InnerRef(Weak<Inner>, LineTy);
                    let inner_ref = InnerRef(inner_weak.clone(), line_ty);

                    // Steal the control from `ScrollWheelMixin`
                    inner.scroll_mixin.stop();
                    inner.table.edit().unwrap().set_display_offset([0.0; 2]);

                    // Temporarily give the control of the scrollbar's value to
                    // `TableScrollbarDragListener`. This flag is reset when
                    // `InnerRef` is dropped.
                    inner.drag_active[line_ty.i()].set(true);

                    impl Drop for InnerRef {
                        fn drop(&mut self) {
                            if let Some(inner) = self.0.upgrade() {
                                inner.drag_active[self.1.i()].set(false);
                            }
                        }
                    }

                    // `TableScrollbarDragListener` uses this closure to borrow
                    // `Table` and `Scrollbar`
                    let accessor = move || {
                        inner_ref.0.upgrade().map(|inner| {
                            let inner2 = Rc::clone(&inner);

                            (
                                OwningRef::new(inner).map(|inner| &inner.table),
                                OwningRef::new(inner2).map(|inner| &inner.scrollbars[line_ty.i()]),
                            )
                        })
                    };

                    Box::new(TableScrollbarDragListener::new(accessor, line_ty)) as _
                } else {
                    // The owner is gone, return a no-op listener
                    Box::new(()) as _
                }
            }));

            // TODO: `set_on_page_step`
        }

        // Watch scroll wheel events
        this.inner.wrapper.set_listener(WrapperViewListener {
            inner: Rc::downgrade(&this.inner),
        });

        this
    }

    /// Get an owned handle to the view representing the widget.
    pub fn view(&self) -> HView {
        self.inner.wrapper.clone()
    }

    /// Borrow the handle to the view representing the widget.
    pub fn view_ref(&self) -> HViewRef<'_> {
        self.inner.wrapper.as_ref()
    }

    /// Get the styling element representing the widget.
    pub fn style_elem(&self) -> HElem {
        self.inner.styled_box.style_elem()
    }

    /// Get a reference to the inner `Table`.
    pub fn table(&self) -> &Table {
        &self.inner.table
    }

    /// Set new size traits (delegated to the inner `Table`).
    ///
    /// Must not have an active edit (the table model must be in the unlocked
    /// state).
    pub fn set_size_traits(&self, value: SizeTraits) {
        self.inner.table.set_size_traits(value);
    }

    /// Set new table flags (delegated to the inner `Table`).
    ///
    /// Must not have an active edit (the table model must be in the unlocked
    /// state).
    pub fn set_flags(&self, value: TableFlags) {
        self.inner.table.set_flags(value);
    }

    /// Set the axes for which scrolling is allowed.
    ///
    /// This might not take effect for an ongoing scroll gesture (if any).
    pub fn set_scrollable_axes(&self, axes: ScrollAxisFlags) {
        self.inner.scroll_mixin.set_axes(axes);
    }

    /// Set the class set of the inner `StyledBox`.
    ///
    /// It defaults to `ClassSet::SCROLL_CONTAINER`. Some bits (e.g.,
    /// `HAS_HORIZONTAL_SCROLLBAR`) are internally enforced and cannot be
    /// modified.
    pub fn set_class_set(&self, mut class_set: ClassSet) {
        let styled_box = &self.inner.styled_box;

        // Protected bits
        let protected = flags![ClassSet::{HAS_HORIZONTAL_SCROLLBAR | HAS_VERTICAL_SCROLLBAR}];
        class_set -= protected;
        class_set |= styled_box.class_set() & protected;
        styled_box.set_class_set(class_set);
    }
}

impl Widget for ScrollableTable {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view_ref()
    }

    fn style_elem(&self) -> Option<HElem> {
        Some(self.style_elem())
    }
}

impl Inner {
    /// Update the internally enforced class sets.
    fn update_class_set(&self) {
        let has_scrollbar = {
            let edit = self.table.edit().unwrap();
            let limits = edit.scroll_limit();

            [limits[0] > 0.0, limits[1] > 0.0]
        };

        let styled_box = &self.styled_box;
        let mut class_set = styled_box.class_set();
        class_set.set(ClassSet::HAS_HORIZONTAL_SCROLLBAR, has_scrollbar[0]);
        class_set.set(ClassSet::HAS_VERTICAL_SCROLLBAR, has_scrollbar[1]);
        if class_set == styled_box.class_set() {
            // no change
            return;
        }
        styled_box.set_class_set(class_set);
    }

    /// Set scrollbar values. Does nothing if a scroll operation is active for
    /// an axis.
    fn update_scrollbar_value(&self) {
        let (values, page_steps) = {
            let edit = self.table.edit().unwrap();

            (
                [
                    table_edit_to_scrollbar_value(&edit, LineTy::Col),
                    table_edit_to_scrollbar_value(&edit, LineTy::Row),
                ],
                [
                    table_edit_to_scrollbar_page_step(&edit, &self.table, LineTy::Col),
                    table_edit_to_scrollbar_page_step(&edit, &self.table, LineTy::Row),
                ],
            )
        };

        for i in 0..2 {
            if !self.drag_active[i].get() {
                self.scrollbars[i].set_value(values[i]);
            }
            self.scrollbars[i].set_page_step(page_steps[i]);
        }
    }
}

struct WrapperViewListener {
    inner: Weak<Inner>,
}

impl WrapperViewListener {
    fn scroll_model_getter(&self) -> impl Fn() -> Box<dyn ScrollModel> + 'static {
        let inner_weak = self.inner.clone();
        move || {
            if let Some(inner) = inner_weak.upgrade() {
                let table = OwningRef::new(inner).map(|inner| &inner.table);
                Box::new(TableScrollModel::new(table).unwrap())
            } else {
                Box::new(())
            }
        }
    }
}

impl ViewListener for WrapperViewListener {
    fn scroll_motion(&self, wm: pal::Wm, _: HViewRef<'_>, _loc: Point2<f32>, delta: &ScrollDelta) {
        if let Some(inner) = self.inner.upgrade() {
            // Do not allow scrolling in two ways at the same time
            if inner.drag_active.iter().any(|x| x.get()) {
                return;
            }

            inner
                .scroll_mixin
                .scroll_motion(wm, delta, self.scroll_model_getter())
        }
    }

    fn scroll_gesture(
        &self,
        _: pal::Wm,
        _: HViewRef<'_>,
        _loc: Point2<f32>,
    ) -> Box<dyn ScrollListener> {
        if let Some(inner) = self.inner.upgrade() {
            // Do not allow scrolling in two ways at the same time
            if inner.drag_active.iter().any(|x| x.get()) {
                return Box::new(());
            }

            inner
                .scroll_mixin
                .scroll_gesture(self.scroll_model_getter())
        } else {
            Box::new(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        testing::{prelude::*, use_testing_wm},
        uicore::HWnd,
    };
    use cggeom::prelude::*;

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn create(twm: &dyn TestingWm) {
        let wm = twm.wm();

        let style_manager = Manager::global(wm);
        let table = Rc::new(ScrollableTable::new(style_manager));

        let wnd = HWnd::new(wm);
        wnd.content_view().set_layout(FillLayout::new(table.view()));
        wnd.set_visibility(true);

        twm.step_unsend();
    }
}
