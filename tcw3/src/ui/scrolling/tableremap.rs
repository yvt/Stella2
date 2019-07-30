//! This module provides a function that assists the management of cell objects
//! (mainly but not limited to subviews) in a table view.
//!
//! A table view displays a subview for each `(row, column)` in the displayed
//! portion of a table model. A table model can insert or remove lines at any
//! moment, and scrolling operations change the viewport. After these changes,
//! we must figure out which subview is still in view and which is not.
//!
//! We assume that the subviews are stored using `ndarray::Array2`. The `Array2`
//! is created based on a viewport and a table model like this: A viewport is a
//! rectangular region `(row1..row2, col1..col2)`. Given a subview creation
//! function `new`, the `Array2` is created by
//! `Array2::from_shape_fn((row2 - row1, col2 - col1), |(r, c)| new(row1 + r, col1 + c))`.
//!
//! We want to create another `Array2` for a slightly modified table model and
//! a different viewport without doing this from scratch. First, we create two
//! instances of `LineIdxMap` (for each set of rows and columns) by passing the
//! line range the current viewport encompasses. Then, we insert and/or remove
//! lines to/from the table model. At the same time, the same operations are
//! done to `LineIdxMap`. We can change the viewport as we wish. After that,
//! we call `LineIdxMap::inverse` to create a mapping from new indices to old
//! indices, which we pass to `shuffle2d` to create the desired `Array2`.
use ndarray::{s, Array2, ArrayViewMut2};
use std::{iter::FusedIterator, ops::Range};

/// Construct a `Array2` by shuffling an existing `ArrayViewMut2`.
///
/// Let `row_src_indices[i]` and `col_src_indices[i]` be the `i`-th element
/// of `row_src_indices` and `col_src_indices`, respectively. The element
/// `out[row, column]` is created by
/// `map(&src[row_src_indices[row], col_src_indices[column]])` if both
/// indices are valid. In other cases, it's created by `new([row, column])`.
///
/// `row_src_indices` and `col_src_indices` are usually created by
/// `LineIdxMap::inverse()`.
pub fn shuffle2d<S, D>(
    mut src: ArrayViewMut2<'_, S>,
    row_src_indices: impl Iterator<Item = usize> + ExactSizeIterator + Clone,
    col_src_indices: impl Iterator<Item = usize> + ExactSizeIterator + Clone,
    mut map: impl FnMut(&mut S) -> D,
    mut new: impl FnMut([usize; 2]) -> D,
) -> Array2<D> {
    let num_rows = row_src_indices.len();
    let num_cols = col_src_indices.len();
    let num_cells = num_cols.checked_mul(num_rows).expect("count overflow");
    let mut cells = Vec::with_capacity(num_cells);

    for (row_dst, row_src) in row_src_indices.take(num_rows).enumerate() {
        if row_src >= src.dim().0 {
            cells.extend((0..num_cols).map(|i| new([row_dst, i])));
        } else {
            let mut row_src = src.slice_mut(s![row_src, ..]);
            let row_src = row_src.as_slice_mut().unwrap();

            let col_src_indices = col_src_indices.clone();
            for (col_dst, col_src) in col_src_indices.take(num_cols).enumerate() {
                if col_src >= row_src.len() {
                    cells.push(new([row_dst, col_dst]));
                } else {
                    cells.push(map(&mut row_src[col_src]));
                }
            }
        }
    }
    assert_eq!(cells.len(), num_cells);

    Array2::from_shape_vec((num_rows, num_cols), cells).unwrap()
}

/// Represents a mapping from lines in *list indices* (elements in a line list,
/// start at `0usize` and correspond to actual memory objects) to
/// *line indices*.
///
/// The mapping tells where each line (column/row) in a table should go after
/// remapping (insertion/removal). A line list is a list of objects representing
/// a particular range of lines. Thus, there is some implicit mapping between
/// elements in the list and line indices, which `LineIdxMap::new` realizes
/// as a `LineIdxMap`.
/// Example 1:
///
/// ```text
///              0  1  2  3  4  5  6  7  8  9  10  11
/// LineIdxMap:                [5  6  7  8  9]
/// ```
///
/// On insertion at range `start..end`, every line at index `i >= start` is
/// pushed back to `end`. There will be no elements in range `start..end`,
/// meaning it has to be filled in by creating brand new objects. Example 2:
///
/// ```text
///              0  1  2  3  4  5  6  7  8  9  10  11
///  inserted:                          [    ]
/// LineIdxMap:                [5  6  7        10  11]
/// ```
///
/// On removal from rang `start..end`, the destination indices in the range are
/// cleared (replaced with `NONE`), meaning their corresponding objects are no
/// longer needed. Example 3:
///
/// ```text
///              0  1  2  3  4  5  -  6  7  8  9  10  11
///  removed:                     [ ]
/// LineIdxMap:                [5  -  6  7  8]
/// ```
///
/// After these transformations, we want to create a new line list from an
/// existing one. Let's say we currently have a list `old_list` for the line
/// range `5..10`, which we want to transform based on Example 2, and from
/// which we want to create a new list `new_list` for the line range `6..11`.
///
/// For each index in range `6..11`, we find a matching element in `LineIdxMap`.
/// For example, the 4th destination index `10` can be found as the 3rd element
/// of `LineIdxMap`, so `new_list[4]` should be filled with `old_list[3]`. On
/// the other hand, the 2th destination index `8` doesn't have a matching
/// element. In this case, `new_list[2]` should be assigned a brand new object.
///
/// ```text
///              0  1  2  3  4  5  6  7  8  9  10  11
///  old_list:                 [5  6  7  8  9]
///                                      |  '-------,
///                                      '------,   |
///  inserted:                          [    ]  |   |
/// LineIdxMap:                [5  6  7        10  11]
///  new_list:                    [6  7  8  9  10]
/// ```
///
/// We need an inverse map to do this efficiently. The `invert` method creates
/// one.
///
/// # Limitations
///
/// `<i64>::min_value()` is not allowed as a line index. However, this is an
/// invalid index anyway because it's a negative value.
#[derive(Debug, Clone)]
pub struct LineIdxMap {
    /// `k == new_line_idx[i]` indicates that the `i`-th element in the original
    /// line list will correspond to the line index `k` after transformation.
    new_line_idx: Vec<i64>,
}

/// A value of `new_line_idx` indicating that the corresponding element is
/// removed during the transformation.
///
/// As the name implies, an idiomatic implementation would use `None` instead.
/// However, I found that the use of this special value greatly simplifies the
/// implementation and improves the generated code's quality and run-time
/// performance.
const NONE: i64 = <i64>::min_value();

impl LineIdxMap {
    /// Construct a `LineIdxMap` with an identity transform for the specified
    /// input viewport range.
    pub fn new(range: Range<i64>) -> Self {
        Self {
            new_line_idx: range.collect(),
        }
    }

    /// Reinitialize `LineIdxMap` with an identity transform. Equivalent to
    /// `new` but may reuse an existing memory allocation.
    pub fn set_identity(&mut self, range: Range<i64>) {
        self.new_line_idx.clear();
        self.new_line_idx.extend(range);
    }

    pub fn insert(&mut self, range: Range<i64>) {
        for line_idx in self.new_line_idx.iter_mut() {
            if *line_idx >= range.start {
                *line_idx += range.end - range.start;
            }
        }
    }

    pub fn remove(&mut self, range: Range<i64>) {
        for line_idx in self.new_line_idx.iter_mut() {
            if range.contains(line_idx) {
                *line_idx = NONE;
            } else if *line_idx >= range.end {
                *line_idx -= range.end - range.start;
            }
        }
    }

    pub fn renew(&mut self, range: Range<i64>) {
        for line_idx in self.new_line_idx.iter_mut() {
            if range.contains(line_idx) {
                *line_idx = NONE;
            }
        }
    }

    /// Construct an inverse map.
    ///
    /// The `i`-th element of the returned iterator tells the original list
    /// index corresponding to the post-transformation line index `vp.start + i`
    /// and the post-transformation list index `i`. `<usize>::max_value()`
    /// indicates that there is no corresponding original list element.
    ///
    /// `vp.end - vp.start` must fit within the range of `isize`.
    pub fn invert(
        &self,
        vp: Range<i64>,
    ) -> impl Iterator<Item = usize> + ExactSizeIterator + Clone + '_ {
        const NONE_LIST_INDEX: usize = <usize>::max_value();

        let count = vp.end.checked_sub(vp.start).expect("count overflow");
        assert!(count < <isize>::max_value() as i64, "count overflow");

        let line_idx_start = vp.start;

        let new_line_idx_it = self.new_line_idx.iter().cloned().enumerate().peekable();

        MapWithState {
            inner: 0..count as usize,
            state: new_line_idx_it,
            mapper: move |i, new_line_idx_it: &mut std::iter::Peekable<_>| {
                let out_line_idx = line_idx_start + i as i64;

                // `new_line_idx` is monotonically increasing (except for `NONE`
                // elements). This fact tells us that a single sweep is
                // sufficient to find an element `(i, line_idx)`in `new_line_idx`
                // such that `line_idx == out_line_idx` for every possible value
                // of `out_line_idx`.

                // Advance `new_line_idx_it` until `line_idx >= out_line_idx`
                while let Some(&(_, line_idx)) = new_line_idx_it.peek() {
                    if line_idx < out_line_idx {
                        new_line_idx_it.next();
                    } else {
                        break;
                    }
                }

                match new_line_idx_it.peek() {
                    Some(&(list_idx, line_idx)) if line_idx == out_line_idx => list_idx,
                    _ => NONE_LIST_INDEX,
                }
            },
        }
    }
}

/// Like `std::iter::Map`, but
#[derive(Debug, Clone)]
struct MapWithState<I, F, T> {
    inner: I,
    mapper: F,
    state: T,
}

impl<I, F, T, O> Iterator for MapWithState<I, F, T>
where
    I: Iterator,
    F: FnMut(I::Item, &mut T) -> O,
{
    type Item = O;

    fn next(&mut self) -> Option<O> {
        self.inner.next().map(|x| (self.mapper)(x, &mut self.state))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I, F, T, O> ExactSizeIterator for MapWithState<I, F, T>
where
    I: ExactSizeIterator,
    F: FnMut(I::Item, &mut T) -> O,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<I, F, T, O> FusedIterator for MapWithState<I, F, T>
where
    I: FusedIterator,
    F: FnMut(I::Item, &mut T) -> O,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Xorshift32(u32);

    impl Xorshift32 {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
        }

        fn next_range(&mut self, range: Range<u32>) -> u32 {
            (self.next() - 1) % (range.end - range.start) + range.start
        }
    }

    #[test]
    fn test_shuffle2d() {
        #[derive(Debug)]
        struct Model {
            // Line model
            rows: Vec<u32>,
            cols: Vec<u32>,
            // Viewport
            row_vp: Range<i64>,
            col_vp: Range<i64>,
        }

        let mut rng = Xorshift32(0x77777777);
        let mut model = Model {
            rows: vec![0, 9, 8, 1, 3, 5, 2, 7, 6, 4],
            cols: vec![4, 9, 2, 5, 8, 7, 6, 0, 1, 3],
            row_vp: 2..7,
            col_vp: 4..9,
        };

        impl Model {
            fn make_table(&self) -> Array2<(u32, u32)> {
                Array2::from_shape_fn(
                    (
                        (self.row_vp.end - self.row_vp.start) as usize,
                        (self.col_vp.end - self.col_vp.start) as usize,
                    ),
                    |(row, col)| self.cell_in_vp(row, col),
                )
            }

            fn cell_in_vp(&self, row: usize, col: usize) -> (u32, u32) {
                (
                    self.rows[row + self.row_vp.start as usize],
                    self.cols[col + self.col_vp.start as usize],
                )
            }
        }

        let mut table = model.make_table();

        use std::cmp::{max, min};
        fn insert(
            rng: &mut Xorshift32,
            lines: &mut Vec<u32>,
            line_idx_map: &mut LineIdxMap,
            range: Range<i64>,
        ) {
            line_idx_map.insert(range.clone());
            for _ in range.clone() {
                lines.insert(range.start as usize, rng.next());
            }
        }
        fn remove(lines: &mut Vec<u32>, line_idx_map: &mut LineIdxMap, range: Range<i64>) {
            let _ = lines[range.start as usize..range.end as usize]; // bounds chk
            line_idx_map.remove(range.clone());
            for _ in range.clone() {
                lines.remove(range.start as usize);
            }
        }
        fn rand_rem_range(rng: &mut Xorshift32, len: i64) -> Range<i64> {
            let p1 = rng.next_range(0..len as u32 + 1) as i64;
            let p2 = rng.next_range(0..len as u32 + 1) as i64;
            min(p1, p2)..max(p1, p2)
        }
        fn rand_ins_range(rng: &mut Xorshift32, len: i64) -> Range<i64> {
            let p = rng.next_range(0..len as u32 + 1) as i64;
            p..p + rng.next_range(0..5) as i64
        }

        for _ in 0..100 {
            {
                let mut row_line_idx_map = LineIdxMap::new(model.row_vp.clone());
                let mut col_line_idx_map = LineIdxMap::new(model.col_vp.clone());

                // Update the line model
                let range = dbg!(rand_rem_range(&mut rng, model.rows.len() as i64));
                remove(&mut model.rows, &mut row_line_idx_map, range);
                let range = dbg!(rand_rem_range(&mut rng, model.cols.len() as i64));
                remove(&mut model.cols, &mut col_line_idx_map, range);
                let range = dbg!(rand_ins_range(&mut rng, model.rows.len() as i64));
                insert(&mut rng, &mut model.rows, &mut row_line_idx_map, range);
                let range = dbg!(rand_ins_range(&mut rng, model.cols.len() as i64));
                insert(&mut rng, &mut model.cols, &mut col_line_idx_map, range);

                let range = dbg!(rand_ins_range(&mut rng, model.rows.len() as i64));
                row_line_idx_map.renew(range);
                let range = dbg!(rand_ins_range(&mut rng, model.cols.len() as i64));
                col_line_idx_map.renew(range);

                // Choose a new viewport
                model.row_vp = rand_rem_range(&mut rng, model.rows.len() as i64);
                model.col_vp = rand_rem_range(&mut rng, model.cols.len() as i64);

                dbg!(&row_line_idx_map);
                dbg!(&col_line_idx_map);
                dbg!(&model);
                dbg!(&table);

                table = shuffle2d(
                    { table }.view_mut(),
                    row_line_idx_map.invert(model.row_vp.clone()),
                    col_line_idx_map.invert(model.col_vp.clone()),
                    // Just copy old elements
                    |&mut x| x,
                    // The cells which were present in the original `table` are
                    // recalculated from the model
                    |[row, col]| model.cell_in_vp(row, col),
                );
            }

            // See if `table` (gradually updated) and `make_table()` (created from
            // scratch) matches
            assert_eq!(table, model.make_table());
        }
    }

    #[test]
    fn inverse_identical() {
        let line_idx_map = LineIdxMap::new(4..12);
        let inv_map: Vec<_> = line_idx_map.invert(4..12).collect();
        assert_eq!(inv_map, (0..8).collect::<Vec<_>>());
    }
}
