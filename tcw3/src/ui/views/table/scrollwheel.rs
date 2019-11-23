//! Low-level scroll event support for `Table`.
use cggeom::{box2, prelude::*, Box2};
use cgmath::{Point2, Vector2};
use std::borrow::Borrow;

use super::{EditLockError, HVp, Table, TableEdit};
use crate::ui::mixins::scrollwheel::ScrollModel;

/// Implements [`ScrollModel`] to bridge between `Table` and `ScrollWheelMixin`.
///
/// [`ScrollModel`]: crate::ui::mixins::scrollwheel::ScrollModel
pub struct TableScrollModel<T: Borrow<Table>> {
    unique_vp: UniqueVp<T>,
    line_size: [f64; 2],
}

struct UniqueVp<T: Borrow<Table>> {
    table: T,
    orig_vp: HVp,
}

impl<T: Borrow<Table>> TableScrollModel<T> {
    /// Construct a `TableScrollModel`.
    pub fn new(table: T) -> Result<Self, EditLockError> {
        // Save the original position
        let orig_vp = {
            let mut edit = table.borrow().edit()?;
            edit.new_vp(edit.scroll_pos())
        };

        Ok(TableScrollModel {
            unique_vp: UniqueVp { table, orig_vp },
            line_size: [15.0; 2],
        })
    }

    /// Construct a modified instance of `TableScrollModel` with new line sizes.
    pub fn with_line_size(self, line_size: [f64; 2]) -> Self {
        Self { line_size, ..self }
    }
}

/// The drop handler for `TableScrollModel`. Assumes that `Table::edit` would
/// succeed.
impl<T: Borrow<Table>> Drop for UniqueVp<T> {
    fn drop(&mut self) {
        let mut edit = self.table.borrow().edit().unwrap();
        edit.remove_vp(self.orig_vp);
    }
}

fn bounds_for_edit(edit: &TableEdit<'_>) -> Box2<f64> {
    let limit = edit.scroll_limit();

    (box2! {
        min: [0.0, 0.0],
        max: limit,
    })
    .translate(-Vector2::from(edit.scroll_pos()))
}

/// Assumes that `Table::edit` would succeed.
impl<T: Borrow<Table>> ScrollModel for TableScrollModel<T> {
    fn bounds(&mut self) -> Box2<f64> {
        let edit = self.unique_vp.table.borrow().edit().unwrap();
        bounds_for_edit(&edit)
    }

    fn pos(&mut self) -> Point2<f64> {
        // `edit.scroll_pos()` is translated to zero (for precision issues with
        // `ScrollWheelMixin`). Note that changing the frame of reference is
        // allowed by `ScrollModel`'s contract.

        let edit = self.unique_vp.table.borrow().edit().unwrap();
        Point2::from(edit.display_offset())
    }

    fn set_pos(&mut self, value: Point2<f64>) {
        let mut edit = self.unique_vp.table.borrow().edit().unwrap();

        let clipped = bounds_for_edit(&edit).limit_point(&value);
        edit.set_scroll_pos((clipped + Vector2::from(edit.scroll_pos())).into());

        // Use the display offset for over-scrolling.
        //
        // It's important that the display offset is set to zero after
        // a scrolling animation is settled. In such a case, `ScrollWheelMixin`
        // passes an in-bound value to this method, meaning `clipped` is equal
        // to `value`. Thus the display offset gets to be zero.
        edit.set_display_offset((value - clipped).into());
    }

    fn line_size(&mut self) -> [f64; 2] {
        self.line_size
    }

    fn cancel(&mut self) {
        let mut edit = self.unique_vp.table.borrow().edit().unwrap();
        edit.set_scroll_pos(edit.vp_pos(self.unique_vp.orig_vp));
    }
}
