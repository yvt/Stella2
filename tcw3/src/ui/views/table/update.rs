use alt_fp::FloatOrd;
use arrayvec::ArrayVec;
use cggeom::Box2;
use cgmath::{Point2, Vector2};
use iterpool::Pool;
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
    DirtyFlags, Inner, LineTy, State, TableCell, TableFlags, TableModelQuery, VpSet,
};
use crate::{
    ui::scrolling::{
        lineset::{DispCb, Index, LinesetModel, Size},
        tableremap::shuffle2d,
    },
    uicore::{HView, HViewRef, Layout, LayoutCtx, SizeTraits},
};

impl Inner {
    /// Call callback functions registered to `model_update_handlers`.
    ///
    /// `state` must be in an unborrowed state (this is a precondition for the
    /// callback functions).
    pub(super) fn call_model_update_handlers(&self) {
        debug_assert!(self.state.try_borrow_mut().is_ok());

        for cb in self.model_update_handlers.borrow().iter() {
            cb();
        }
    }

    /// Call callback functions registered to `prearrange_handlers`.
    ///
    /// `state` must be in an unborrowed state (this is a precondition for the
    /// callback functions).
    pub(super) fn call_prearrange_handlers(&self) {
        debug_assert!(self.state.try_borrow_mut().is_ok());

        for cb in self.prearrange_handlers.borrow().iter() {
            cb();
        }
    }

    /// An utility function for updating `self.dirty`.
    pub(super) fn set_dirty_flags(&self, new_flags: DirtyFlags) {
        self.dirty.set(self.dirty.get() | new_flags);
    }

    /// Update `State::cells`, clearing the dirty flag `CELLS`. Might set
    /// the dirty flag `LAYOUT`.
    ///
    /// Returns `true` iff the state is updated, i.e., iff the dirty flag
    /// `CELLS` had been set.
    ///
    /// If the result `call_model_update_handlers` is `true`, the caller usually
    /// has to call `call_model_update_handlers` as well.
    pub(super) fn update_cells(&self, state: &mut State) -> bool {
        if !self.dirty.get().contains(DirtyFlags::CELLS) {
            return false;
        }
        self.dirty.set(self.dirty.get() - DirtyFlags::CELLS);
        self.dirty.set(self.dirty.get() | DirtyFlags::LAYOUT);

        // Regroup line groups. This makes sure every line group in the viewport
        // correspond to a single line.
        for &ty in &[LineTy::Col, LineTy::Row] {
            let size = self.size.get()[ty.i()];
            let lineset = &mut state.linesets[ty.i()];

            // Regrouping might shrink some line groups. A set of line groups
            // that covered the viewport might no longer after regrouping. If
            // this happens, we try regrouping again.
            loop {
                // Bound the viewport offset first
                state.vp_set.bound_by(ty, lineset.total_size(), size);

                // Calculate the viewport range
                let vp_ranges = state.vp_set.vp_ranges(ty, size);

                struct DispCbImpl<'a> {
                    line_ty: LineTy,
                    vp_set: &'a mut VpSet,
                    vp_size: Size,
                }

                impl DispCb for DispCbImpl<'_> {
                    fn line_resized(
                        &mut self,
                        _range: Range<Index>,
                        old_pos: Range<Size>,
                        new_pos: Range<Size>,
                    ) {
                        // Apply the displacement policy
                        self.vp_set.adjust_vp_for_line_resizing(
                            self.line_ty,
                            self.vp_size,
                            old_pos,
                            new_pos,
                        );
                    }
                }

                let lineset_model = LinesetModelImpl::new(&mut *state.model_query, ty);
                let mut disp_cb = DispCbImpl {
                    line_ty: ty,
                    vp_set: &mut state.vp_set,
                    vp_size: self.size.get()[ty.i()],
                };

                lineset.regroup(&lineset_model, &vp_ranges, &mut disp_cb);

                let new_vp_ranges = state.vp_set.vp_ranges(ty, size);

                if new_vp_ranges
                    .iter()
                    .all(|vp| lineset.is_well_grouped(vp.clone()).0)
                {
                    break;
                }
            }
        }

        // Calculate the range of visible lines
        #[allow(clippy::reversed_empty_ranges)]
        let mut new_cells_ranges = [0..0, 0..0];
        for &ty in &[LineTy::Col, LineTy::Row] {
            let size = self.size.get()[ty.i()];

            let vp = state.vp_set.primary_vp_range(ty, size);

            let (_line_grs, line_grs_range_idx, _line_grs_range_pos) =
                state.linesets[ty.i()].range(vp);

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
            |[col, row]| {
                let col = col as u64 + new_cells_ranges[0].start as u64;
                let row = row as u64 + new_cells_ranges[1].start as u64;
                let (view, ctrler) = model_query.new_view([col, row]);
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

        true
    }

    pub(super) fn update_layout_if_needed(this: &Rc<Inner>, state: &State, view: HViewRef<'_>) {
        // Return if `LAYOUT` is not set.
        // `LAYOUTING` menas we are currently in `TableLayout::arrange`, so we
        // can't call `HViewRef::set_layout`.
        if !this.dirty.get().contains(DirtyFlags::LAYOUT)
            || this.dirty.get().contains(DirtyFlags::LAYOUTING)
        {
            return;
        }
        this.dirty.set(this.dirty.get() - DirtyFlags::LAYOUT);

        view.set_layout(TableLayout::from_current_state(Rc::clone(&this), state));
    }
}

impl VpSet {
    pub(super) fn new() -> Self {
        let mut pool = Pool::new();

        // Create a primary viewport
        let ptr = pool.allocate([0; 2]);

        assert_eq!(ptr, super::primary_vp_ptr());

        Self { vp_pool: pool }
    }

    /// Adjust viewports after some lines are resized.
    ///
    /// This is where the so-called displacement policy is implemented.
    /// TODO: provide means to customize the displacement policy.
    ///
    /// Does not update dirty flags.
    pub(super) fn adjust_vp_for_line_resizing(
        &mut self,
        line_ty: LineTy,
        vp_size: Size,
        old_pos: Range<Size>,
        new_pos: Range<Size>,
    ) {
        debug_assert!(old_pos.start == new_pos.start);

        for vp in self.vp_pool.iter_mut() {
            let vp = &mut vp[line_ty.i()];

            // Fix the right/bottom edge
            let bottom = *vp + vp_size;

            if old_pos.end <= bottom {
                let diff = new_pos.end - old_pos.end;
                *vp = max(0, *vp + diff);
            } else if old_pos.start < bottom {
                // The resized line set includes the right/bottom edge. Move the
                // viewport so that resizing won't reveal the next line.
                *vp = max(0, min(*vp, new_pos.end - vp_size));
            }
        }
    }

    /// Restrict viewport positions by the total size of lines.
    fn bound_by(&mut self, line_ty: LineTy, total_size: Size, vp_size: Size) {
        debug_assert!(total_size >= 0);
        debug_assert!(vp_size >= 0);

        let max_vp = total_size - vp_size;

        for vp in self.vp_pool.iter_mut() {
            let vp = &mut vp[line_ty.i()];

            *vp = max(0, min(*vp, max_vp));
        }
    }

    /// Get the viewport for the primary viewport aka the scroll
    /// position.
    fn primary_vp_range(&self, line_ty: LineTy, vp_size: Size) -> Range<Size> {
        let vp = &self.vp_pool[super::primary_vp_ptr()];

        Self::to_vp_range(vp, line_ty, vp_size)
    }

    /// Calculate a range for a given viewport.
    #[allow(clippy::wrong_self_convention)]
    fn to_vp_range(vp: &[Size; 2], line_ty: LineTy, vp_size: Size) -> Range<Size> {
        let vp = vp[line_ty.i()];
        vp..vp + vp_size
    }

    /// Get a list of viewports.
    fn vp_ranges(
        &self,
        line_ty: LineTy,
        vp_size: Size,
    ) -> impl std::ops::Deref<Target = [Range<Size>]> {
        self.vp_pool
            .iter()
            .map(|vp| Self::to_vp_range(vp, line_ty, vp_size))
            .collect::<Vec<_>>()
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
        }

        // Call prearrange handlers
        self.inner
            .dirty
            .set(self.inner.dirty.get() | DirtyFlags::LAYOUTING);
        self.inner.call_prearrange_handlers();
        self.inner
            .dirty
            .set(self.inner.dirty.get() - DirtyFlags::LAYOUTING);

        if self.inner.dirty.get().contains(DirtyFlags::CELLS) {
            self.inner.update_cells(&mut self.inner.state.borrow_mut());
        }

        if self.inner.dirty.get().contains(DirtyFlags::LAYOUT) {
            self.inner.call_model_update_handlers();

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
        for ((col, row), view) in self.subviews.indexed_iter() {
            let cell = [col, row];

            // Get the corner coordinates
            let mut min = Point2::new(0.0, 0.0);
            let mut max = Point2::new(0.0, 0.0);
            for &ty in &[LineTy::Col, LineTy::Row] {
                min[ty.i()] = self.pos_lists[ty.i()][cell[ty.i()]];
                max[ty.i()] = self.pos_lists[ty.i()][cell[ty.i()] + 1];
            }

            ctx.set_subview_frame(view.as_ref(), Box2::new(min, max));
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
        let flags = inner.flags.get();

        // Get coordinates of the lines
        let pos_lists: ArrayVec<[_; 2]> = [LineTy::Col, LineTy::Row]
            .iter()
            .map(|&ty| {
                let i = ty as usize;
                let cells_range = &state.cells_ranges[i];
                let lineset = &state.linesets[i];
                let display_offset = state.display_offset[i] as f32;

                let vp_size = inner.size.get()[ty.i()];
                let vp = state.vp_set.primary_vp_range(ty, vp_size);

                let (mut line_grs, line_grs_range_idx, line_grs_range_pos) =
                    lineset.range(vp.clone());

                assert!(line_grs_range_idx.start <= cells_range.start);

                let mut i = line_grs_range_idx.start;
                let mut pos = line_grs_range_pos.start - vp.start;

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

                // Grow the last line if `GROW_LAST_*` is specified
                if flags
                    .contains([TableFlags::GROW_LAST_COL, TableFlags::GROW_LAST_ROW][ty as usize])
                    && i == lineset.num_lines()
                    && pos_list.len() > 1
                {
                    let pos = pos_list.last_mut().unwrap();
                    *pos = pos.fmax(fix_to_f32(vp_size));
                }

                // Add the display offset
                for x in pos_list.iter_mut() {
                    *x -= display_offset;
                }

                pos_list
            })
            .collect();

        // Create an array of `HView` (`subviews()` needs a slice of them)
        // TODO: We could skip if this was in `State`. But then `shuffle2d`
        //       won't do anymore...
        let cells = &state.cells;
        let subviews = Array2::from_shape_fn(cells.dim(), |i| cells[i].view.clone());

        Self {
            subviews,
            inner,
            pos_lists: pos_lists.into_inner().unwrap(),
        }
    }
}
