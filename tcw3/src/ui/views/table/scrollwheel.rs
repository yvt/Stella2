//! Low-level scroll event support for `Table`.
use cggeom::{box2, Box2};
use cgmath::Point2;
use std::borrow::Borrow;

use super::{EditLockError, HVp, Table};
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

/// Assumes that `Table::edit` would succeed.
impl<T: Borrow<Table>> ScrollModel for TableScrollModel<T> {
    fn bounds(&mut self) -> Box2<f64> {
        let edit = self.unique_vp.table.borrow().edit().unwrap();
        let limit = edit.scroll_limit();

        box2! {
            min: [0.0, 0.0],
            max: limit,
        }
    }

    fn pos(&mut self) -> Point2<f64> {
        let edit = self.unique_vp.table.borrow().edit().unwrap();
        edit.scroll_pos().into()
    }

    fn set_pos(&mut self, value: Point2<f64>) {
        let mut edit = self.unique_vp.table.borrow().edit().unwrap();
        edit.set_scroll_pos(value.into());
        // TODO: `set_scroll_pos` automatically clips the value, so some
        //       animations do not work correctly
    }

    fn line_size(&mut self) -> [f64; 2] {
        self.line_size
    }

    fn cancel(&mut self) {
        let mut edit = self.unique_vp.table.borrow().edit().unwrap();
        edit.set_scroll_pos(edit.vp_pos(self.unique_vp.orig_vp));
    }
}
