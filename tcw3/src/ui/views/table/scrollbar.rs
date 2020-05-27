//! Low-level scrollbar support for `Table`.
use alt_fp::FloatOrd;
use cggeom::prelude::*;
use std::cell::Cell;

use super::{HVp, LineTy, Table, TableEdit};
use crate::{
    pal,
    ui::{
        scrolling::piecewise::piecewise_map,
        views::scrollbar::{ScrollbarDragListener, ScrollbarRaw},
    },
};

/// Maintains the state data required for translating a scrollbar drag operation
/// into scrolling of a `Table`.
#[derive(Debug, Default)]
pub struct TableScrollbarDragState {
    initial: Cell<Option<Point>>,
}

#[derive(Debug, Clone, Copy)]
struct Point {
    /// The viewport that pins the original scroll position.
    vp: HVp,
    /// The original scrollbar value.
    value: f64,
}

impl TableScrollbarDragState {
    /// Construct a `TableScrollbarDragState`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Handles [`ScrollbarDragListener::down`].
    ///
    /// [`ScrollbarDragListener::down`]: crate::ui::views::scrollbar::ScrollbarDragListener::down
    pub fn down(&self, sb: &ScrollbarRaw, edit: &mut TableEdit<'_>, _line_ty: LineTy) {
        assert!(self.initial.get().is_none());

        let vp = edit.new_vp(edit.scroll_pos());
        let value = sb.value();

        self.initial.set(Some(Point { vp, value }));
    }

    /// Handles [`ScrollbarDragListener::motion`].
    ///
    /// [`ScrollbarDragListener::motion`]: crate::ui::views::scrollbar::ScrollbarDragListener::motion
    pub fn motion(
        &self,
        sb: &ScrollbarRaw,
        edit: &mut TableEdit<'_>,
        line_ty: LineTy,
        new_value: f64,
    ) {
        let initial = self.initial.get().unwrap();

        // Construct a piecewise linear mapping between scroll values and
        // scroll positions for smooth experience
        let mut endpoints = [
            (0.0, 0.0),
            (sb.value(), edit.scroll_pos()[line_ty.i()]),
            (initial.value, edit.vp_pos(initial.vp)[line_ty.i()]),
            (1.0, edit.scroll_limit()[line_ty.i()]),
        ];

        if endpoints[1] > endpoints[2] {
            endpoints.swap(1, 2);
        }

        // Map the new scroll value to a scroll position
        let new_scroll_pos = piecewise_map(endpoints.iter().copied(), new_value);

        let mut vp_pos = edit.scroll_pos();
        vp_pos[line_ty.i()] = new_scroll_pos;
        edit.set_scroll_pos(vp_pos);

        sb.set_value(new_value);
    }

    /// Handles [`ScrollbarDragListener::cancel`].
    ///
    /// [`ScrollbarDragListener::cancel`]: crate::ui::views::scrollbar::ScrollbarDragListener::cancel
    pub fn cancel(&self, sb: &ScrollbarRaw, edit: &mut TableEdit<'_>, line_ty: LineTy) {
        if let Some(point) = self.initial.take() {
            // Restore the original scroll position
            let mut vp_pos = edit.scroll_pos();
            vp_pos[line_ty.i()] = edit.vp_pos(point.vp)[line_ty.i()];
            edit.set_scroll_pos(vp_pos);

            sb.set_value(point.value);

            edit.remove_vp(point.vp);
        }
    }

    /// Handles [`ScrollbarDragListener::up`].
    ///
    /// [`ScrollbarDragListener::up`]: crate::ui::views::scrollbar::ScrollbarDragListener::up
    pub fn up(&self, sb: &ScrollbarRaw, edit: &mut TableEdit<'_>, line_ty: LineTy) {
        let point = self.initial.take().unwrap();
        edit.remove_vp(point.vp);

        // Reset the mapping between scroll values and scroll positions to
        // the one represented by `table_edit_to_scrollbar_value`
        sb.set_value(table_edit_to_scrollbar_value(edit, line_ty));
    }
}

/// Wraps [`TableScrollbarDragState`] and implements [`ScrollbarDragListener`].
/// `A` is used to get references to a `Table` and `ScrollbarRaw` which are to
/// be bound.
#[derive(Debug)]
pub struct TableScrollbarDragListener<A> {
    accessor: A,
    line_ty: LineTy,
    state: TableScrollbarDragState,
}

impl<A> TableScrollbarDragListener<A> {
    /// Construct a `TableScrollbarDragListener`.
    pub fn new(accessor: A, line_ty: LineTy) -> Self {
        Self {
            accessor,
            line_ty,
            state: TableScrollbarDragState::new(),
        }
    }

    /// Get a reference to the contained `A`.
    pub fn accessor_ref(&self) -> &A {
        &self.accessor
    }
}

impl<A, T, S> ScrollbarDragListener for TableScrollbarDragListener<A>
where
    A: Fn() -> Option<(T, S)>,
    T: std::ops::Deref<Target = Table>,
    S: std::ops::Deref<Target = ScrollbarRaw>,
{
    fn down(&self, _: pal::Wm, _new_value: f64) {
        if let Some((table, sb)) = (self.accessor)() {
            let mut edit = table.edit().unwrap();
            self.state.down(&sb, &mut edit, self.line_ty);
        }
    }

    fn motion(&self, _: pal::Wm, new_value: f64) {
        if let Some((table, sb)) = (self.accessor)() {
            let mut edit = table.edit().unwrap();
            self.state.motion(&sb, &mut edit, self.line_ty, new_value);
        }
    }

    fn up(&self, _: pal::Wm) {
        if let Some((table, sb)) = (self.accessor)() {
            let mut edit = table.edit().unwrap();
            self.state.up(&sb, &mut edit, self.line_ty);
        }
    }

    fn cancel(&self, _: pal::Wm) {
        if let Some((table, sb)) = (self.accessor)() {
            let mut edit = table.edit().unwrap();
            self.state.cancel(&sb, &mut edit, self.line_ty);
        }
    }
}

/// Convert the scroll position of a given `TableEdit` to a scrollbar value.
///
/// **This method must not be used** if there is an active scroll operation such
/// as [`TableScrollbarDragState`] because it may use a different mapping.
pub fn table_edit_to_scrollbar_value(edit: &TableEdit<'_>, line_ty: LineTy) -> f64 {
    // should be some finite value in `0.0..1.0` if the RHS (`scroll_limit`) is
    // zero
    let x = edit.scroll_pos()[line_ty.i()]
        / edit.scroll_limit()[line_ty.i()].fmax(std::f64::MIN_POSITIVE);

    debug_assert!(x >= 0.0 && x <= 1.0, "0 ≤ {:?} ≤ 1", x);

    x
}

/// Calculate the page step size for a given `TableEdit` and `Table`.
pub fn table_edit_to_scrollbar_page_step(
    edit: &TableEdit<'_>,
    table: &Table,
    line_ty: LineTy,
) -> f64 {
    // The page step can be infinity if the table is not scrollable for a given
    // axis. However, it must not be NaN.
    (table.view().frame().size()[line_ty.i()] as f64).fmax(std::f64::MIN_POSITIVE)
        / edit.scroll_limit()[line_ty.i()]
}
