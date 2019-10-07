//! Implements the public interface of `TableEdit`.
use std::{
    cell::RefMut,
    cmp::{max, min},
    mem::ManuallyDrop,
    ops::Range,
    rc::Rc,
};

use super::{
    fixedpoint::{fix_to_fp, fp_to_fix},
    update::LinesetModelImpl,
    DirtyFlags, Inner, LineTy, State, TableModelEdit, TableModelQuery, VpSet,
};
use crate::{
    ui::scrolling::lineset::{DispCb, Index, Size},
    uicore::HView,
};

/// A lock guard type for updating a [`Table`]'s internal representation of a
/// table model and viewports.
///
/// This type is constructed by [`Table::edit`].
///
/// [`Table`]: super::Table
/// [`Table::edit`]: super::Table::edit
#[derive(Debug)]
pub struct TableEdit<'a> {
    pub(super) view: &'a HView,
    pub(super) inner: &'a Rc<Inner>,
    pub(super) state: ManuallyDrop<RefMut<'a, State>>,
}

impl Drop for TableEdit<'_> {
    fn drop(&mut self) {
        // Process pending updates and clear dirty flags, which might have been
        // set by editing operations
        let did_model_update = self.inner.update_cells(&mut self.state);
        Inner::update_layout_if_needed(&self.inner, &self.state, self.view);

        // Unborrow `state` before calling the callback functions
        unsafe {
            ManuallyDrop::drop(&mut self.state);
        }

        if did_model_update {
            self.inner.call_model_update_handlers();
        }
    }
}

impl TableEdit<'_> {
    /// Get the current scrolling position for a given axis.
    pub fn scroll_pos(&self, line_ty: LineTy) -> f64 {
        let primary_vp = self.state.vp_set.vp_pool[super::primary_vp_ptr()];
        fix_to_fp(primary_vp[line_ty.i()])
    }

    /// Set the current scrolling position for a given axis.
    ///
    /// `pos` is automatically clamped to range `0..scroll_limit(line_ty)`.
    pub fn set_scroll_pos(&mut self, line_ty: LineTy, pos: f64) {
        let new_pos = max(0, min(fp_to_fix(pos), self.scroll_limit_raw(line_ty)));

        let primary_vp = &mut self.state.vp_set.vp_pool[super::primary_vp_ptr()];
        let vp = &mut primary_vp[line_ty.i()];

        if new_pos != *vp {
            *vp = new_pos;
            self.inner.set_dirty_flags(DirtyFlags::CELLS);
        }
    }

    /// Get the maximum scrolling position (the maximum value for `scroll_pos`)
    /// for a given axis.
    pub fn scroll_limit(&self, line_ty: LineTy) -> f64 {
        fix_to_fp(self.scroll_limit_raw(line_ty))
    }

    fn scroll_limit_raw(&self, line_ty: LineTy) -> Size {
        let lineset = &self.state.linesets[line_ty.i()];
        let content_size = lineset.total_size();

        let vp_size = self.inner.size.get()[line_ty.i()];

        max(0, content_size - vp_size)
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
        let state = &mut **self.state;

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
        state.vp_set.adjust_vp_for_line_resizing(
            line_ty,
            self.inner.size.get()[line_ty.i()],
            pos_range.start..pos_range.start,
            pos_range.clone(),
        );

        self.inner.set_dirty_flags(DirtyFlags::CELLS);
    }

    fn remove(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut **self.state;

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
        state.vp_set.adjust_vp_for_line_resizing(
            line_ty,
            self.inner.size.get()[line_ty.i()],
            pos_range.clone(),
            pos_range.start..pos_range.start,
        );

        self.inner.set_dirty_flags(DirtyFlags::CELLS);
    }

    fn resize(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut **self.state;

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

                self.inner.set_dirty_flags(DirtyFlags::CELLS);
            }
        }

        let lineset_model = LinesetModelImpl::new(&mut *state.model_query, line_ty);
        let mut disp_cb = DispCbImpl {
            line_ty,
            inner: &self.inner,
            vp_set: &mut state.vp_set,
            vp_size: self.inner.size.get()[line_ty.i()],
        };
        let skip_approx = false;
        lineset.recalculate_size(&lineset_model, range, skip_approx, &mut disp_cb);
    }

    fn renew_subviews(&mut self, line_ty: LineTy, range: Range<u64>) {
        let state = &mut **self.state;

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
