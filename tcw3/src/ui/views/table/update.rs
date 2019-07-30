use arrayvec::ArrayVec;
use cggeom::{prelude::*, Box2};
use cgmath::{Point2, Vector2};
use ndarray::Array2;
use std::{cell::RefCell, ops::Range, rc::Rc};

use super::{
    fixedpoint::{fix_to_f32, fp_to_fix},
    Inner, LineTy, TableModelQuery,
};
use crate::{
    ui::scrolling::lineset::{Index, LinesetModel, Size},
    uicore::{HView, Layout, LayoutCtx, SizeTraits},
};

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

/// A `Layout` implementation for `Table`.
///
/// It refers various fields in `inner`, but `Layout` is required to be
/// logically immutable. This means that `TableLayout` must be recreated from
/// scratch on many occasions, even if none of `TableLayout`'s fields have to
/// be updated.
pub(super) struct TableLayout {
    subviews: Array2<HView>,
    inner: Rc<Inner>,
    pos_lists: [Vec<f32>; 2],
}

impl Layout for TableLayout {
    fn subviews(&self) -> &[HView] {
        self.subviews.as_slice().unwrap()
    }

    fn size_traits(&self, _ctx: &LayoutCtx) -> SizeTraits {
        self.inner.size_traits.get()
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        // If `size` changes, we have to recalculate the visible line sets.
        let fix_size = size.cast::<f64>().unwrap().map(fp_to_fix);
        if fix_size != self.inner.size.get() {
            self.inner.size.set(fix_size);
            // TODO: do the recalculation thingy

            // Set a new layout, restarting the layout process
            ctx.set_layout(Self::from_current_state(Rc::clone(&self.inner)));
            return;
        }

        // Arrange subviews
        for ((row, col), view) in self.subviews.indexed_iter() {
            let cell = [row, col];

            // Get the corner coordinates
            let mut min = Point2::new(0.0, 0.0);
            let mut max = Point2::new(0.0, 0.0);
            for &ty in &[LineTy::Row, LineTy::Col] {
                *ty.point_get_mut(&mut min) = self.pos_lists[ty.i()][cell[ty.i()]];
                *ty.point_get_mut(&mut max) = self.pos_lists[ty.i()][cell[ty.i()] + 1];
            }

            ctx.set_subview_frame(view, Box2::new(min, max));
        }
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        use as_any::Downcast;
        if let Some(_other) = (*other).downcast_ref::<Self>() {
            // TODO: Add a version number to `cells` so that this can be
            //       checked fast
            false
        } else {
            false
        }
    }
}

impl TableLayout {
    /// Construct a `TableLayout` based on the current state of a table view.
    /// The constructed `TableLayout` might recreate itself as the view varies
    /// in its size. That's why it needs a `Rc<Inner>`.
    ///
    /// `inner.state` must not have a mutable borrow at the point of the
    /// function call.
    pub(super) fn from_current_state(inner: Rc<Inner>) -> Self {
        // TODO: Assert `line_idx_maps` is an identity transform
        let state = inner.state.borrow();

        // Get coordinates of the lines
        let pos_lists: ArrayVec<[_; 2]> = [LineTy::Row, LineTy::Col]
            .iter()
            .map(|&ty| {
                let i = ty as usize;
                let cells_range = &state.cells_ranges[i];
                let lineset = &state.linesets[i];

                let vp_start = inner.vp[i].get();
                let vp_end = vp_start + *ty.vec_get(&inner.size.get());

                let (mut line_grs, line_grs_range_idx, line_grs_range_pos) =
                    lineset.range(vp_start..vp_end);

                assert!(line_grs_range_idx.start <= cells_range.start);

                let mut i = line_grs_range_idx.start;
                let mut pos = line_grs_range_pos.start - vp_start;

                // Skip some lines if `line_grs` extra lines
                while i < cells_range.start {
                    let (size, num_lines) = line_grs.next().unwrap();
                    pos += size;
                    i += num_lines;
                }

                assert!(i <= cells_range.start);

                // Calculate the line coordinates in range
                let num_lines = (cells_range.end - cells_range.start) as usize;
                let mut pos_list = Vec::with_capacity(num_lines + 1);
                pos_list.push(fix_to_f32(pos));

                while i < cells_range.end {
                    let (size, num_lines) = line_grs.next().unwrap();
                    assert_eq!(num_lines, 1);

                    pos += size;
                    i += num_lines;

                    pos_list.push(fix_to_f32(pos));
                }

                assert_eq!(pos_list.len(), pos_list.capacity());

                pos_list
            })
            .collect();

        // Create an array of `HView` (`subviews()` needs a slice of them)
        // TODO: We could skip if this was in `State`. But then `shuffle2d`
        //       won't do anymore...
        let cells = &state.cells;
        let subviews = Array2::from_shape_fn(cells.dim(), |i| cells[i].view.clone());

        drop(state);

        Self {
            subviews,
            inner,
            pos_lists: pos_lists.into_inner().unwrap(),
        }
    }
}
