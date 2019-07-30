//! Implements the public interface of `TableEdit`.
use std::{cell::RefMut, ops::Range, rc::Rc};

use super::{
    update::LinesetModelImpl, DirtyFlags, Inner, LineTy, State, TableModelEdit, TableModelQuery,
};
use crate::{
    ui::scrolling::lineset::{DispCb, Index, Size},
    uicore::HView,
};

/// A lock guard type for updating a [`Table`]'s internal representation of a
/// table model.
///
/// This type is constructed by [`Table::edit`].
///
/// [`Table`]: super::Table
/// [`Table::edit`]: super::Table::edit
#[derive(Debug)]
pub struct TableEdit<'a> {
    pub(super) view: &'a HView,
    pub(super) inner: &'a Rc<Inner>,
    pub(super) state: RefMut<'a, State>,
}

impl Drop for TableEdit<'_> {
    fn drop(&mut self) {
        // Process pending updates and clear dirty flags, which might have been
        // set by editing operations
        self.inner.update_cells(&mut self.state);
        Inner::update_layout_if_needed(&self.inner, &self.state, self.view);
    }
}

impl TableModelEdit for TableEdit<'_> {
    fn model_mut(&mut self) -> &mut dyn TableModelQuery {
        &mut *self.state.model_query
    }

    fn set_model_boxed(&mut self, new_model: Box<dyn TableModelQuery>) {
        self.state.model_query = new_model;
    }

    fn insert(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut *self.state;

        let lineset = &mut state.linesets[line_ty.i()];
        let line_idx_maps = &mut state.line_idx_maps[line_ty.i()];

        let range = range.start as i64..range.end as i64;
        assert!(
            range.start >= 0 && range.start <= lineset.num_lines(),
            "invalid insertion point {}. valid range is 0..={}",
            range.start,
            lineset.num_lines()
        );

        if range.start >= range.end {
            return;
        }

        line_idx_maps.insert(range.clone());

        let lineset_model = LinesetModelImpl::new(&mut *state.model_query, line_ty);
        let pos_range = lineset.insert(&lineset_model, range).unwrap();

        // Apply the displacement policy
        self.inner.adjust_vp_for_line_resizing(
            line_ty,
            pos_range.start..pos_range.start,
            pos_range.clone(),
        );

        self.inner.set_dirty_flags(DirtyFlags::CELLS);
    }

    fn remove(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut *self.state;

        let lineset = &mut state.linesets[line_ty.i()];
        let line_idx_maps = &mut state.line_idx_maps[line_ty.i()];

        let range = range.start as i64..range.end as i64;
        assert!(
            range.start >= 0 && range.end <= lineset.num_lines(),
            "invalid removal range {:?}. valid range is 0..{}",
            range,
            lineset.num_lines()
        );

        if range.start >= range.end {
            return;
        }

        line_idx_maps.remove(range.clone());

        let lineset_model = LinesetModelImpl::new(&mut *state.model_query, line_ty);
        let pos_range = lineset.remove(&lineset_model, range).unwrap();

        // Apply the displacement policy
        self.inner.adjust_vp_for_line_resizing(
            line_ty,
            pos_range.clone(),
            pos_range.start..pos_range.start,
        );

        self.inner.set_dirty_flags(DirtyFlags::CELLS);
    }

    fn resize(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut *self.state;

        let lineset = &mut state.linesets[line_ty.i()];

        let range = range.start as i64..range.end as i64;
        assert!(
            range.start >= 0 && range.end <= lineset.num_lines(),
            "invalid removal range {:?}. valid range is 0..{}",
            range,
            lineset.num_lines()
        );

        if range.start >= range.end {
            return;
        }

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

                self.inner.set_dirty_flags(DirtyFlags::CELLS);
            }
        }

        let lineset_model = LinesetModelImpl::new(&mut *state.model_query, line_ty);
        let mut disp_cb = DispCbImpl {
            line_ty,
            inner: self.inner,
        };
        let skip_approx = false;
        lineset.recalculate_size(&lineset_model, range, skip_approx, &mut disp_cb);
    }

    fn renew_subviews(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut *self.state;

        let lineset = &mut state.linesets[line_ty.i()];
        let line_idx_maps = &mut state.line_idx_maps[line_ty.i()];

        let range = range.start as i64..range.end as i64;
        assert!(
            range.start >= 0 && range.end <= lineset.num_lines(),
            "invalid removal range {:?}. valid range is 0..{}",
            range,
            lineset.num_lines()
        );

        if range.start >= range.end {
            return;
        }

        line_idx_maps.renew(range);

        self.inner.set_dirty_flags(DirtyFlags::CELLS);
    }
}
