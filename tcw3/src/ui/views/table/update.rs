use std::{cell::RefCell, ops::Range};

use super::{fixedpoint::fp_to_fix, LineTy, TableModelQuery};
use crate::ui::scrolling::lineset::{Index, LinesetModel, Size};

/// Exposes `TableModelQuery` as a `LinesetModel`.
pub(super) struct LinesetModelImpl<'a> {
    // TODO: Modify `LinesetModel::line_total_size` to accept `&mut self` so
    //       that we can remove this `RefCell`
    model_query: RefCell<&'a mut dyn TableModelQuery>,

    /// A single `TableModelQuery` object provides a size model for both axes.
    /// Thus, this field specifies which axis is currently being concerned with.
    line_ty: LineTy,
}

impl<'a> LinesetModelImpl<'a> {
    pub(super) fn new(model_query: &'a mut dyn TableModelQuery, line_ty: LineTy) -> Self {
        Self {
            model_query: RefCell::new(model_query),
            line_ty,
        }
    }
}

impl LinesetModel for LinesetModelImpl<'_> {
    fn line_total_size(&self, range: Range<Index>, approx: bool) -> Size {
        debug_assert!(range.start >= 0);
        debug_assert!(range.end >= 0);

        let size = self.model_query.borrow_mut().range_size(
            self.line_ty,
            range.start as u64..range.end as u64,
            approx,
        );

        fp_to_fix(size)
    }
}
