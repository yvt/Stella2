//! Implements the public interface of `TableEdit`.
use std::ops::Range;

use super::{update::LinesetModelImpl, LineTy, TableEdit, TableModelEdit, TableModelQuery};
use crate::ui::scrolling::lineset::{DispCb, Index, Size};

impl Drop for TableEdit<'_> {
    fn drop(&mut self) {
        // TODO: Remap `cells` using `line_idx_maps`, etc.
        // TODO: Add and check a dirty flag for each `LineIdxMap`
        // TODO: Update the view's layout
    }
}

impl TableModelEdit for TableEdit<'_> {
    fn model_mut(&mut self) -> &mut dyn TableModelQuery {
        &mut *self.state.model_query
    }

    fn set_model(&mut self, new_model: Box<dyn TableModelQuery>) {
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

        // TODO: update viewports using `pos_range`
        let _ = pos_range;
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

        // TODO: update viewports using `pos_range`
        let _ = pos_range;
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

        struct DispCbImpl {}

        impl DispCb for DispCbImpl {
            fn line_resized(
                &mut self,
                _range: Range<Index>,
                _old_pos: Range<Size>,
                _new_pos: Range<Size>,
            ) {
                // TODO: update viewports
            }
        }

        let lineset_model = LinesetModelImpl::new(&mut *state.model_query, line_ty);
        let mut disp_cb = DispCbImpl {};
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
    }
}
