use arrayvec::ArrayVec;
use cggeom::{prelude::*, Box2};
use cgmath::{Point2, Vector2};
use ndarray::Array2;
use std::{
    cell::RefCell,
    cmp::{max, min},
    mem::replace,
    ops::Range,
    rc::Rc,
};

use super::{
    fixedpoint::{fix_to_f32, fp_to_fix},
    DirtyFlags, Inner, LineTy, State, TableCell, TableModelQuery,
};
use crate::{
    ui::scrolling::{
        lineset::{DispCb, Index, LinesetModel, Size},
        tableremap::shuffle2d,
    },
    uicore::{HView, Layout, LayoutCtx, SizeTraits},
};

impl Inner {
    /// Adjust viewports after some lines are resized.
    ///
    /// This is where the so-called displacement policy is implemented.
    /// TODO: provide means to customize the displacement policy.
    ///
    /// Does not update dirty flags.
    pub(super) fn adjust_vp_for_line_resizing(
        &self,
        line_ty: LineTy,
        old_pos: Range<Size>,
        new_pos: Range<Size>,
    ) {
        debug_assert!(old_pos.start == new_pos.start);

        let size = *line_ty.vec_get(&self.size.get());

        let vp_cell = &self.vp[line_ty.i()];
        let mut vp = vp_cell.get();

        // Fix the right/bottom edge
        let bottom = vp_cell.get() + size;

        if old_pos.end <= bottom {
            let diff = new_pos.end - old_pos.end;
            vp = max(0, vp + diff);
        } else if old_pos.start < bottom {
            // The resized line set includes the right/bottom edge. Move the
            // viewport so that resizing won't reveal the next line.
            vp = max(0, min(vp, new_pos.end - size));
        }

        vp_cell.set(vp);
    }

    /// An utility function for updating `self.dirty`.
    pub(super) fn set_dirty_flags(&self, new_flags: DirtyFlags) {
        self.dirty.set(self.dirty.get() | new_flags);
    }

    /// Update `State::cells`, clearing the dirty flag `CELLS`. Might set
    /// the dirty flag `LAYOUT`.
    pub(super) fn update_cells(&self, state: &mut State) {
        if !self.dirty.get().contains(DirtyFlags::CELLS) {
            return;
        }
        self.dirty.set(self.dirty.get() - DirtyFlags::CELLS);
        self.dirty.set(self.dirty.get() | DirtyFlags::LAYOUT);

        // Regroup line groups. This makes sure every line group in the viewport
        // correspond to a single line.
        for &ty in &[LineTy::Row, LineTy::Col] {
            let size = *ty.vec_get(&self.size.get());
            let vp_cell = &self.vp[ty.i()];
            let lineset = &mut state.linesets[ty.i()];

            // Regrouping might shrink some line groups. A set of line groups
            // that covered the viewport might no longer after regrouping. If
            // this happens, we try regrouping again.
            loop {
                // Bound the viewport offset first
                let max_vp = lineset.total_size() - size;
                vp_cell.set(max(0, min(vp_cell.get(), max_vp)));

                // Calculate the viewport range
                let vp_start = vp_cell.get();
                let vp = vp_start..vp_start + size;

                struct DispCbImpl<'a> {
                    line_ty: LineTy,
                    inner: &'a Inner,
                }

                impl DispCb for DispCbImpl<'_> {
                    fn line_resized(
                        &mut self,
                        _range: Range<Index>,
                        old_pos: Range<Size>,
                        new_pos: Range<Size>,
                    ) {
                        // Apply the displacement policy
                        self.inner
                            .adjust_vp_for_line_resizing(self.line_ty, old_pos, new_pos);
                    }
                }

                let lineset_model = LinesetModelImpl::new(&mut *state.model_query, ty);
                let mut disp_cb = DispCbImpl {
                    line_ty: ty,
                    inner: self,
                };

                lineset.regroup(&lineset_model, &[vp.clone()], &mut disp_cb);

                if lineset.is_well_grouped(vp.clone()).0 {
                    break;
                }
            }
        }

        // Calculate the range of visible lines
        let mut new_cells_ranges = [0..0, 0..0];
        for &ty in &[LineTy::Row, LineTy::Col] {
            let size = *ty.vec_get(&self.size.get());

            let vp_cell = &self.vp[ty.i()];
            let vp_start = vp_cell.get();
            let vp_end = vp_start + size;

            let (_line_grs, line_grs_range_idx, _line_grs_range_pos) =
                state.linesets[ty.i()].range(vp_start..vp_end);

            new_cells_ranges[ty.i()] = line_grs_range_idx;
        }

        // Remap `cells` using the new `cells_ranges`.
        //
        // We do not wish to re-create `cells` from scratch. We should be able
        // to simply move elements from the old `cells` for table cells that
        // remained on the screen. This is where `line_idx_maps` comes in.
        // See `tableremap`'s module documentation for details.
        let model_query = &mut state.model_query;
        let new_cells = shuffle2d(
            state.cells.view_mut(),
            state.line_idx_maps[0].invert(new_cells_ranges[0].clone()),
            state.line_idx_maps[1].invert(new_cells_ranges[1].clone()),
            // Map function (for existing cells)
            |old_cell: &mut TableCell| TableCell {
                view: old_cell.view.clone(),
                ctrler: replace(&mut old_cell.ctrler, Box::new(())),
            },
            // Factory function (for new cells)
            |[row, col]| {
                let row = row as u64 + new_cells_ranges[0].start as u64;
                let col = col as u64 + new_cells_ranges[1].start as u64;
                let (view, ctrler) = model_query.new_view([row, col]);
                TableCell { view, ctrler }
            },
        );

        state.cells = new_cells;
        state.cells_ranges = new_cells_ranges;

        // Reset `line_idx_maps`.
        for (line_idx_map, cells_range) in state
            .line_idx_maps
            .iter_mut()
            .zip(state.cells_ranges.iter())
        {
            line_idx_map.set_identity(cells_range.clone());
        }
    }

    pub(super) fn update_layout_if_needed(this: &Rc<Inner>, state: &State, view: &HView) {
        if !this.dirty.get().contains(DirtyFlags::LAYOUT) {
            return;
        }
        this.dirty.set(this.dirty.get() - DirtyFlags::LAYOUT);

        view.set_layout(TableLayout::from_current_state(Rc::clone(&this), state));
    }
}

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

            self.inner.set_dirty_flags(DirtyFlags::CELLS);
            self.inner.update_cells(&mut self.inner.state.borrow_mut());

            // The `LAYOUT` dirty flag can be cleared here because
            // we'll replace layouts this instant
            self.inner
                .dirty
                .set(self.inner.dirty.get() - DirtyFlags::LAYOUT);

            // Set a new layout, restarting the layout process
            ctx.set_layout(Self::from_current_state(
                Rc::clone(&self.inner),
                &self.inner.state.borrow(),
            ));
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
    pub(super) fn from_current_state(inner: Rc<Inner>, state: &State) -> Self {
        // TODO: Assert `line_idx_maps` is an identity transform

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
