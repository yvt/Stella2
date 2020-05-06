use alt_fp::FloatOrd;
use boxed_slice_tools::repeating_default;
use cggeom::Box2;
use cgmath::{vec2, Point2, Vector2};
use std::{cell::RefCell, cmp::max};

use crate::{
    ui::AlignFlags,
    uicore::{HView, Layout, LayoutCtx, SizeTraits},
};

/// A `Layout` that arranges subviews on imaginary table cells.
#[derive(Debug, Clone)]
pub struct TableLayout {
    /// Each element corresponds to the element in `items` with an identical
    /// index. `HView`s are stored in a separate `Vec` because
    /// `Layout::subviews` wants `&[HView]`.
    subviews: Box<[HView]>,
    items: Box<[Item]>,
    margin: [f32; 4],

    columns: Box<[Line]>,
    rows: Box<[Line]>,

    state: RefCell<State>,
}

#[derive(Debug, Clone)]
struct Item {
    cell: [usize; 2],
    align: AlignFlags,
}

#[derive(Debug, Clone)]
struct State {
    // The following two fields stores cached values.
    columns: Box<[LineState]>,
    rows: Box<[LineState]>,

    /// A temporary storage used by `solve_lines`.
    ///
    /// (Ideally it should be `alloca`-ed instead, but it's gonna be a long way
    /// before it can be done in Rust)
    clearances: Box<[Clearance]>,
}

/// Represents a row or column's static data.
#[derive(Debug, Clone, Default)]
struct Line {
    /// The number of items in the line.
    num_items: usize,

    /// The spacing for the right or bottom edge of the line. Must be zero for
    /// the last (rightmost or bottommost) lines.
    spacing: f32,
}

/// Represents a row or column's dynamic data.
#[derive(Debug, Clone)]
struct LineState {
    // The size traits for the line, calculated by `size_traits`.
    size_min: f32,
    size_max: f32,
    size_preferred: f32,

    /// The actual position of the right/bottom edge of the line,
    /// calculated by `arrange`.
    pos: f32,
}

impl Default for LineState {
    #[inline]
    fn default() -> Self {
        Self {
            size_min: 0.0,
            size_max: std::f32::INFINITY,
            size_preferred: 0.0,
            pos: 0.0,
        }
    }
}

impl std::iter::Sum for LineState {
    #[inline]
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(
            LineState {
                size_min: 0.0,
                size_max: 0.0,
                size_preferred: 0.0,
                pos: 0.0,
            },
            |a, b| LineState {
                size_min: a.size_min + b.size_min,
                size_max: a.size_max + b.size_max,
                size_preferred: a.size_preferred + b.size_preferred,
                pos: a.pos + b.pos,
            },
        )
    }
}

/// Stores the amount by which the corresponding line can be expanded/shrunken.
#[derive(Debug, Clone, Default)]
#[repr(align(8))] // A 16-byte block is faster to copy than a 12-byte block
struct Clearance {
    /// The index of the line.
    index: usize,
    /// The amount by which the corresponding line can be expanded/shrunken.
    amount: f32,
}

impl TableLayout {
    /// Construct a `TableLayout` from a set of tuples `(view, cell, align)`.
    pub fn new(cells: impl IntoIterator<Item = (HView, [usize; 2], AlignFlags)>) -> Self {
        let (subviews, items): (Vec<_>, Vec<_>) = cells
            .into_iter()
            .map(|(view, cell, align)| (view, Item { cell, align }))
            .unzip();

        Self::new_inner(subviews.into(), items.into())
    }

    /// Construct a `TableLayout` from a set of tuples `(view, align)`, stacking
    /// the subviews horizontally from left to right.
    pub fn stack_horz(cells: impl IntoIterator<Item = (HView, AlignFlags)>) -> Self {
        Self::new(
            cells
                .into_iter()
                .enumerate()
                .map(|(i, (view, align))| (view, [i, 0], align)),
        )
    }

    /// Construct a `TableLayout` from a set of tuples `(view, align)`, stacking
    /// the subviews vertically from top to bottom.
    pub fn stack_vert(cells: impl IntoIterator<Item = (HView, AlignFlags)>) -> Self {
        Self::new(
            cells
                .into_iter()
                .enumerate()
                .map(|(i, (view, align))| (view, [0, i], align)),
        )
    }

    fn new_inner(subviews: Box<[HView]>, items: Box<[Item]>) -> Self {
        let num_columns = items.iter().map(|item| item.cell[0] + 1).max().unwrap_or(0);
        let num_rows = items.iter().map(|item| item.cell[1] + 1).max().unwrap_or(0);

        // Count items in each line
        let mut columns: Box<[Line]> = repeating_default(num_columns);
        let mut rows: Box<[Line]> = repeating_default(num_rows);
        for item in items.iter() {
            columns[item.cell[0]].num_items += 1;
            rows[item.cell[1]].num_items += 1;
        }

        Self {
            subviews,
            items,
            margin: [0.0; 4],
            columns,
            rows,
            state: RefCell::new(State {
                columns: repeating_default(num_columns),
                rows: repeating_default(num_rows),
                clearances: repeating_default(max(num_columns, num_rows)),
            }),
        }
    }

    /// Update the margin value with a single value used for all four edges and
    /// return a new `TableLayout`, consuming `self`.
    pub fn with_uniform_margin(self, margin: f32) -> Self {
        Self {
            margin: [margin; 4],
            ..self
        }
    }

    /// Update the margin value with four values used for respective edges and
    /// return a new `TableLayout`, consuming `self`.
    pub fn with_margin(self, margin: [f32; 4]) -> Self {
        Self { margin, ..self }
    }

    /// Update the inter-cell spacing value and return a new `TableLayout`,
    /// consuming `self`.
    pub fn with_uniform_spacing(mut self, spacing: f32) -> Self {
        if let Some((_, lines)) = self.rows.split_last_mut() {
            for line in lines.iter_mut() {
                line.spacing = spacing;
            }
        }
        if let Some((_, lines)) = self.columns.split_last_mut() {
            for line in lines.iter_mut() {
                line.spacing = spacing;
            }
        }
        self
    }

    /// Update the inter-cell spacing value for each column divider and return a
    /// new `TableLayout`, consuming `self`.
    ///
    /// `spacing.len() + 1` must be equal to the number of the columns in the
    /// layout.
    pub fn with_column_spacing(mut self, spacing: &[f32]) -> Self {
        debug_assert_eq!(spacing.len() + 1, self.columns.len());
        for (line, &spacing) in self.columns.iter_mut().zip(spacing.iter()) {
            line.spacing = spacing;
        }
        self
    }

    /// Update the inter-cell spacing value for each row divider and return a
    /// new `TableLayout`, consuming `self`.
    ///
    /// `spacing.len() + 1` must be equal to the number of the rows in the
    /// layout.
    pub fn with_row_spacing(mut self, spacing: &[f32]) -> Self {
        debug_assert_eq!(spacing.len() + 1, self.rows.len());
        for (line, &spacing) in self.rows.iter_mut().zip(spacing.iter()) {
            line.spacing = spacing;
        }
        self
    }

    /// Update the inter-cell spacing value for a column divider in-place.
    ///
    /// `i + 1` must be less than the number of the columns in the layout.
    pub fn set_column_spacing(&mut self, i: usize, spacing: f32) {
        debug_assert!(i + 1 < self.columns.len());
        self.columns[i].spacing = spacing;
    }

    /// Update the inter-cell spacing value for a row divider in-place.
    ///
    /// `i + 1` must be less than the number of the rows in the layout.
    pub fn set_row_spacing(&mut self, i: usize, spacing: f32) {
        debug_assert!(i + 1 < self.rows.len());
        self.rows[i].spacing = spacing;
    }

    pub fn num_rows(&self) -> usize {
        self.rows.len()
    }

    pub fn num_columns(&self) -> usize {
        self.columns.len()
    }
}

impl Layout for TableLayout {
    fn subviews(&self) -> &[HView] {
        &self.subviews
    }

    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // Enable split borrow

        // Recalculate each line's size traits
        for line_st in state.columns.iter_mut() {
            *line_st = LineState::default();
        }
        for line_st in state.rows.iter_mut() {
            *line_st = LineState::default();
        }

        for (view, item) in self.subviews.iter().zip(self.items.iter()) {
            let st = ctx.subview_size_traits(view.as_ref());

            // Some `AlignFlags` relaxes the size traits
            let st = item.align.containing_size_traits(st);

            let column = &mut state.columns[item.cell[0]];
            column.size_min = column.size_min.fmax(st.min.x);
            column.size_max = column.size_max.fmin(st.max.x);
            column.size_preferred += st.preferred.x;

            let row = &mut state.rows[item.cell[1]];
            row.size_min = row.size_min.fmax(st.min.y);
            row.size_max = row.size_max.fmin(st.max.y);
            row.size_preferred += st.preferred.y;
        }

        fn postproc_line(line_sts: &mut [LineState], lines: &[Line]) {
            for (line_st, line) in line_sts.iter_mut().zip(lines.iter()) {
                if line.num_items > 0 {
                    line_st.size_preferred /= line.num_items as f32;
                    line_st.size_max = line_st.size_max.fmax(line_st.size_min);
                    line_st.size_preferred = line_st
                        .size_preferred
                        .fmax(line_st.size_min)
                        .fmin(line_st.size_max);
                } else {
                    // Ignore empty lines as if they didn't exist at all.
                    line_st.size_max = 0.0;
                }
            }
        }
        postproc_line(&mut state.columns, &self.columns);
        postproc_line(&mut state.rows, &self.rows);

        // Check the invariant of `spacing`
        debug_assert_eq!(
            self.columns.last().map(|line| line.spacing).unwrap_or(0.0),
            0.0
        );
        debug_assert_eq!(
            self.rows.last().map(|line| line.spacing).unwrap_or(0.0),
            0.0
        );

        // Return a `SizeTraits` based on the lines' size traits
        let margin = self.margin;
        let extra = vec2(margin[1] + margin[3], margin[0] + margin[2])
            + vec2(
                self.columns.iter().map(|line| line.spacing).sum(),
                self.rows.iter().map(|line| line.spacing).sum(),
            );

        let row_sum: LineState = state.rows.iter().cloned().sum();
        let column_sum: LineState = state.columns.iter().cloned().sum();

        SizeTraits {
            min: vec2(column_sum.size_min, row_sum.size_min) + extra,
            max: vec2(column_sum.size_max, row_sum.size_max) + extra,
            preferred: vec2(column_sum.size_preferred, row_sum.size_preferred) + extra,
        }
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // Enable split borrow

        // Determine the actual size of every column and row
        let margin = self.margin;
        let origin = [margin[3], margin[0]];
        let extra = vec2(margin[1] + margin[3], margin[0] + margin[2])
            + vec2(
                self.columns.iter().map(|line| line.spacing).sum(),
                self.rows.iter().map(|line| line.spacing).sum(),
            );
        solve_lines(&mut state.columns, size.x - extra.x, &mut state.clearances);
        solve_lines(&mut state.rows, size.y - extra.y, &mut state.clearances);

        apply_spacing(&self.columns, &mut state.columns, origin[0]);
        apply_spacing(&self.rows, &mut state.rows, origin[1]);

        // Arrange subviews
        for (view, item) in self.subviews.iter().zip(self.items.iter()) {
            let cell = item.cell;
            let cell_box = Box2::new(
                Point2::new(
                    cell[0]
                        .checked_sub(1)
                        .map(|i| state.columns[i].pos + self.columns[i].spacing)
                        .unwrap_or(origin[0]),
                    cell[1]
                        .checked_sub(1)
                        .map(|i| state.rows[i].pos + self.rows[i].spacing)
                        .unwrap_or(origin[1]),
                ),
                Point2::new(state.columns[cell[0]].pos, state.rows[cell[1]].pos),
            );

            let st = ctx.subview_size_traits(view.as_ref());

            let subview_frame = item.align.arrange_child(&cell_box, &st);

            ctx.set_subview_frame(view.as_ref(), subview_frame);
        }
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        if let Some(other) = as_any::Downcast::downcast_ref::<Self>(other) {
            self.subviews == other.subviews
        } else {
            false
        }
    }
}

/// Determine the given lines' final sizes by formulating it as a quadratic
/// optimization problem.
///
/// This function updates `LineState::pos`. `clearances` is used as a temporary
/// storage.
///
/// The score function is the MSE of line sizes relative to their preferred
/// sizes. Their sizes are bounded by their min/max sizes.
fn solve_lines(lines: &mut [LineState], total_size: f32, clearances: &mut [Clearance]) {
    // How much do we have to expand/shrink the lines based on their preferred size?
    let total_preferred: f32 = lines.iter().map(|l| l.size_preferred).sum();
    let goal_increment = total_size - total_preferred;

    // Throughout this process, `LineState::pos` is temporarily used to store
    // the actual size of the corresponding line.

    // Calculate the clearance for each line. Sort by their amount.
    //
    // `f32` isn't `Ord`, so `amount` can't be directly used as a sort key.
    // `amount` is in range `[0, ∞]`, and we don't care ordering between
    // infinity elements. So they can be sorted efficiently by reinterpreting
    // the binary32 encoding of `amount` as `u32`.
    let clearances = &mut clearances[0..lines.len()];
    for (i, (line, clearance)) in lines.iter_mut().zip(clearances.iter_mut()).enumerate() {
        clearance.index = i;
        if goal_increment > 0.0 {
            clearance.amount = line.size_max - line.size_preferred;
        } else {
            clearance.amount = line.size_preferred - line.size_min;
        }

        // The following assertion should hold because of the rigorous clamping
        // done in `postproc_line`.
        debug_assert!(clearance.amount >= 0.0);
    }

    // Usually, the number of lines is fairly low (< 16), so a simple insertion
    // sort sufficies (and may be actually faster)
    minisort::insertion_sort_by_key(clearances, |c| c.amount.to_bits());

    // Expand/shrink lines uniformly. When some line gets saturated, i.e., hits
    // the clearance (the maximum delta allowed by the size traits), remove it
    // from consideration. Repeat this until either the goal amount is reached
    // or there are no more lines to expand/shrink.
    let mut num_saturated = 0;
    let mut size_delta = 0.0; // The delta for every unsaturated line
    let mut remaining_increment = goal_increment.abs();

    while num_saturated < lines.len() {
        let num_unsaturated = lines.len() - num_saturated;
        let new_size_delta = size_delta + remaining_increment / num_unsaturated as f32;

        // Does this get any line saturated?
        let next_clearance = clearances[num_saturated].amount;
        if new_size_delta <= next_clearance {
            // No, the algorithm is finished.

            // Distribute `remaining_increment` evenly to the unsaturated line set.
            for c in clearances[num_saturated..].iter() {
                let line = &mut lines[c.index];
                line.pos = line.size_preferred + new_size_delta.copysign(goal_increment);
            }

            // The saturated line set is handled right after the loop.
            break;
        }

        // The line is saturated. We expanded/shrunk the total width by amount
        // less than `remaining_increment` in this step.
        remaining_increment -= (next_clearance - size_delta) * num_unsaturated as f32;
        size_delta = next_clearance;

        // Add the line to the saturated line set. We proceed the algorithm
        // using the remaining, unsaturated line set.
        num_saturated += 1;
    }

    // Update `pos` of the saturated line set.
    for c in clearances[0..num_saturated].iter() {
        let line = &mut lines[c.index];
        if goal_increment > 0.0 {
            line.pos = line.size_max;
        } else {
            line.pos = line.size_min;
        }
    }

    // Finalize `LineState::pos`
    let mut sum = 0.0;
    for line in lines.iter_mut() {
        sum += line.pos;
        line.pos = sum;
    }
}

/// Apply margin and inter-cell spacing values.
fn apply_spacing(lines: &[Line], line_states: &mut [LineState], origin: f32) {
    let mut offset = origin;
    for (line, line_state) in lines.iter().zip(line_states.iter_mut()) {
        line_state.pos += offset;
        offset += line.spacing;
    }
}
