use derive_more::{Add, AddAssign, Neg};
use rope::{self, Rope};
use std::{
    cmp::max,
    ops::{Range, RangeInclusive},
};

/// The type for representing line sizes and positions.
///
/// Positions start at `0`. This type is defined as a signed integer because
/// it's also used to represent differences.
///
/// Positions are real values. We don't use floating-point types because `Rope`
/// does not like numerical errors.
pub type Size = i64;

/// The type for representing line indices.
///
/// Indices start at `0`. This type is defined as a signed integer because
/// it's also used to represent differences.
pub type Index = i64;

/// A lineset is a data structure used by a table view to track the heights of
/// lines and/or their approximation.
///
/// The heights of off-screen lines are tracked in groups of multiple units
/// (called *line group*), increasing in size as they get distant from the
/// visible portion. Lines inside the visible portion are tracked at the full,
/// per-line granularity.
#[derive(Debug, Clone)]
pub struct Lineset {
    /// A list of line groups, each comprising of one or more lines.
    line_grs: Rope<LineGr, LineOff>,
    /// A list of LOD groups sorted in the ascending order of indices. Each
    /// element defines the starting point of the corresponding LOD group.
    /// `lod_grs[0].index` must be `0` so that this encompasses entire the
    /// lineset.
    ///
    /// This is empty iff the lineset includes zero lines.
    lod_grs: Vec<LodGr>,
}

pub trait LinesetModel {
    /// Get the total size of the lines in the specified range. The result may
    /// be approximate if `approx` is `true`.
    ///
    /// If `approx` is `false`, `range.end - range.start` must be equal to `1`.
    fn line_total_size(&self, range: Range<Index>, approx: bool) -> Size;
}

/// Represents a line group.
#[derive(Debug, Clone, Copy)]
struct LineGr {
    num_lines: Index,
    /// The total size of lines in the line group. Can be approximate only if
    /// the line group belongs to a LOD group with a non-zero LOD.
    size: Size,
}

/// The rope offset type for `LineGr`.
#[derive(Debug, Clone, Copy, Add, AddAssign, Neg)]
struct LineOff {
    index: Index,
    pos: Size,
}

impl LineOff {
    fn index(&self) -> Index {
        self.index
    }
}

impl rope::Offset for LineOff {
    fn zero() -> Self {
        Self { index: 0, pos: 0 }
    }
}

impl rope::ToOffset<LineOff> for LineGr {
    fn to_offset(&self) -> LineOff {
        LineOff {
            index: self.num_lines,
            pos: self.size,
        }
    }
}

/// Defines the starting point of a LOD group.
///
/// Each LOD group is populated by one or more line groups. It's associated with
/// a LOD value `lod`, which dictates the size of every line group in the LOD
/// group.
///
/// ```text
///                                              visible portion
///  LOD groups:                                      <-->
///  ,------------+-------------------------+--------+----+--------+-------------,
///  | 3          | 2                       | 1      | 0  | 1      | 2           |
///  '------------+-------------------------+--------+----+--------+-------------'
///  line groups:
///  ,------------+----+----+----+----+----++--+--+--++++++--+--+--+----+----+---,
///  |            |    |    |    |    |    ||  |  |  ||||||  |  |  |    |    |   |
///  '------------+----+----+----+----+----++--+--+--++++++--+--+--+----+----+---'
///
/// ```
#[derive(Debug, Clone, Copy)]
struct LodGr {
    index: Index,
    lod: u8,
}

/// Get the valid line group size range for the specified LOD.
fn lod_size_range(lod: u8) -> RangeInclusive<Index> {
    let shift1 = lod as u32 - (lod > 0) as u32; // max(lod - 1, 0)
    let shift2 = lod as u32;
    (1 << shift1)..=(1 << shift2)
}

/// Get the minimum LOD that can contain the specified line group size.
fn min_lod_for_size(size: Index) -> u8 {
    debug_assert!(size >= 1);
    ((0 as Index).leading_zeros() - (size - 1).leading_zeros()) as u8
}

/// Divide a size into two. This function ensures that the total size remains
/// unchanged.
fn divide_size(size: Size, ratio: [Size; 2]) -> [Size; 2] {
    let size1 = (size as f64 * ratio[0] as f64 / (ratio[0] + ratio[1]) as f64 + 0.5) as Size;
    [size1, size - size1]
}

impl Lineset {
    pub fn new() -> Self {
        Self {
            line_grs: Rope::new(),
            lod_grs: Vec::new(),
        }
    }

    /// Synchronize the structure after new lines are inserted to the underlying
    /// model (`LinesetModel`).
    ///
    /// The time complexity of this operation is logarithmic, provided that
    /// `regroup` is called after each operation.
    pub fn insert(&mut self, model: &dyn LinesetModel, range: Range<Index>) {
        if range.end <= range.start {
            return;
        }
        assert!(range.start <= self.line_grs.offset_len().index);
        assert!(range.start >= 0);

        let mut num_lines = range.end - range.start;

        if range.start == self.line_grs.offset_len().index {
            // Create a new LOD group.
            // If this happens repeatedly, the length of `lod_grs` would be
            // O(n). However, `insert` isn't supposed to be used like that.
            let lod = min_lod_for_size(num_lines);
            self.lod_grs.push(LodGr {
                index: self.line_grs.offset_len().index,
                lod,
            });
            self.line_grs.push_back(LineGr {
                num_lines,
                size: model.line_total_size(range, lod == 0),
            });
            return;
        }

        // Find the LOD group the new lines belong to
        let lod_gr_i = match self.lod_grs.binary_search_by_key(&range.start, |g| g.index) {
            Ok(i) => i,
            Err(i) => i - 1,
        };

        let lod = self.lod_grs[lod_gr_i].lod;
        let lod_size_range = lod_size_range(lod);

        // Find the line group the new lines are inserted to
        use rope::{by_key, range_by_key, Edge::Floor, One::FirstAfter, ToOffset};
        let (line_gr, line_gr_off) = {
            let (mut iter, range) = self
                .line_grs
                .range(range_by_key(LineOff::index, Floor(range.start)..));
            (iter.nth(0).unwrap().clone(), range.start)
        };

        // Endpoints of the line group (pre-insertion)
        let line_gr_start = line_gr_off.index;
        let line_gr_end = line_gr_start + line_gr.num_lines;

        let next;

        if range.start != line_gr_start || num_lines < *lod_size_range.start() {
            debug_assert!(lod > 0);

            // The total size of the new lines
            let size = model.line_total_size(range.clone(), lod > 0);

            // The new lines fall in the middle of an existing line group.
            // Or, the new lines are so few that they cannot constitute a line
            // group by themselves.
            if *lod_size_range.end() - line_gr.num_lines >= num_lines {
                // Insert the new lines to the existing line group.
                self.line_grs.update_with(
                    FirstAfter(by_key(LineOff::index, line_gr_start)),
                    |line_gr, _| {
                        line_gr.num_lines += num_lines;
                        line_gr.size += size;
                    },
                );

                // `range` was completely assimilated
                next = None;
            } else if *lod_size_range.end() * 2 - line_gr.num_lines >= num_lines {
                // Insert the new lines to the existing line group, and then
                // divide it into two to satisfy the invariant.
                let new_gr_num_lines = line_gr.num_lines + num_lines;
                let new_gr_mid = line_gr_start + (new_gr_num_lines >> 1);

                let halve_sizes_new;
                if range.start > new_gr_mid {
                    // Divide `line_gr` at `new_gr_mid`.
                    let halve_sizes_old = divide_size(
                        line_gr.size,
                        [
                            model.line_total_size(line_gr_start..new_gr_mid, lod > 0),
                            model.line_total_size(new_gr_mid..range.start, lod > 0)
                                + model
                                    .line_total_size(range.end..line_gr_end + num_lines, lod > 0),
                        ],
                    );

                    // The new lines belongs to the second half
                    halve_sizes_new = [halve_sizes_old[0], halve_sizes_old[1] + size];
                } else if range.end > new_gr_mid {
                    // Divide `line_gr` at `new_gr_mid`.
                    let halve_sizes_old = divide_size(
                        line_gr.size,
                        [
                            model.line_total_size(line_gr_start..range.start, lod > 0),
                            model.line_total_size(range.end..line_gr_end + num_lines, lod > 0),
                        ],
                    );

                    // The new lines are split into both halves
                    let size2 = [
                        model.line_total_size(range.start..new_gr_mid, lod > 0),
                        model.line_total_size(new_gr_mid..range.end, lod > 0),
                    ];
                    halve_sizes_new =
                        [halve_sizes_old[0] + size2[0], halve_sizes_old[1] + size2[1]];
                } else {
                    // Divide `line_gr` at `new_gr_mid`.
                    let halve_sizes_old = divide_size(
                        line_gr.size,
                        [
                            model.line_total_size(line_gr_start..range.start, lod > 0)
                                + model.line_total_size(range.end..new_gr_mid, lod > 0),
                            model.line_total_size(new_gr_mid..line_gr_end + num_lines, lod > 0),
                        ],
                    );

                    // The new lines belongs to the first half
                    halve_sizes_new = [halve_sizes_old[0] + size, halve_sizes_old[1]];
                }

                // `line_gr` will be the second half
                self.line_grs
                    .update_with(
                        FirstAfter(by_key(LineOff::index, line_gr_start)),
                        |line_gr, _| {
                            line_gr.num_lines = line_gr_end + num_lines - new_gr_mid;
                            line_gr.size = halve_sizes_new[1];
                        },
                    )
                    .unwrap();

                // ... and insert the first half before that
                self.line_grs
                    .insert_before(
                        LineGr {
                            num_lines: new_gr_mid - line_gr_start,
                            size: halve_sizes_new[0],
                        },
                        FirstAfter(by_key(LineOff::index, line_gr_start)),
                    )
                    .unwrap();

                // `range` was completely assimilated
                next = None;
            } else {
                // The existing line group, combined with the new lines, does
                // not fit in two line groups.

                // The above two conditions were not met, which implies:
                debug_assert!(num_lines > *lod_size_range.end());
                debug_assert!(line_gr.num_lines + num_lines > *lod_size_range.end() * 2);
                // Combined with the fact that `lod > 0`, this means:
                debug_assert!(line_gr.num_lines + num_lines > *lod_size_range.start() * 4);
                // (This overpopulated line group can be broken into at least
                // three line groups.)

                // We will split the line group into two at `range.start`.
                // Depending on the split position, this might create one or two
                // underpopulated line groups. To resolve this state, we move
                // some lines from `range` (the new lines) to these line groups.
                // After this adjustment, the number of lines in `range` is
                // calculated as:
                //
                //     line_gr.num_lines + num_lines - max(i, lod_size_range.start())
                //         - max(line_gr.num_lines - i, lod_size_range.start())
                //     (where i == range.start - line_gr_start)
                //
                // It can be shown that this is greater than or equal to
                // `lod_size_range.start()`, thus it's still enough to
                // constitute a line group of a LOD `lod`.

                // How many lines are moved from `range` to each half?
                let adj_num_lines = [
                    max(0, *lod_size_range.start() - (range.start - line_gr_start)),
                    max(0, *lod_size_range.start() - (line_gr_end - range.start)),
                ];

                // After the adjustment (removal of lines), this is the new
                // `range`:
                let new_range = (range.start + adj_num_lines[0])..(range.end - adj_num_lines[1]);

                debug_assert!(new_range.end - new_range.start >= *lod_size_range.start());

                // Divide `line_gr` at `range.start`.
                let halve_sizes = divide_size(
                    line_gr.size,
                    [
                        model.line_total_size(line_gr_start..range.start, lod > 0),
                        model.line_total_size(range.end..line_gr_end + num_lines, lod > 0),
                    ],
                );

                // Apply the adjustment to `halve_sizes`
                let halve_sizes_postadj = [
                    halve_sizes[0] + model.line_total_size(range.start..new_range.start, lod > 0),
                    halve_sizes[1] + model.line_total_size(new_range.end..range.end, lod > 0),
                ];

                // `line_gr` will be the second half
                self.line_grs
                    .update_with(
                        FirstAfter(by_key(LineOff::index, line_gr_start)),
                        |line_gr, _| {
                            line_gr.num_lines = line_gr_end - range.start + adj_num_lines[1];
                            line_gr.size = halve_sizes_postadj[1];
                        },
                    )
                    .unwrap();

                // ... and insert the first half before `line_gr`
                self.line_grs
                    .insert_before(
                        LineGr {
                            num_lines: range.start - line_gr_start + adj_num_lines[0],
                            size: halve_sizes_postadj[0],
                        },
                        FirstAfter(by_key(LineOff::index, line_gr_start)),
                    )
                    .unwrap();

                // The total size of `new_range`
                let new_size = model.line_total_size(new_range.clone(), lod > 0);

                // Update the following LOD groups' starting indices
                // (This could be merged with the last `for` statement, but that
                // will complicate the insertion routine)
                let incr = adj_num_lines[0] + adj_num_lines[1];
                if incr > 0 {
                    for lod_gr in self.lod_grs[lod_gr_i + 1..].iter_mut() {
                        lod_gr.index += incr;
                    }
                }
                num_lines -= incr;

                next = Some((new_range, Some(new_size)));
            }
        } else {
            next = Some((range, None));
        }

        let mut lod_gr_i2 = lod_gr_i;

        if let Some((range2, size2)) = next {
            // Insert `range2` (which is a non-strict subrange of `range`)
            // between/before/after existing line group(s)
            debug_assert!(range2.end - range2.start >= *lod_size_range.start());

            // `range2` must fit in a single line group. Choose the minimum LOD
            // for that. If
            let lod2 = max(lod, min_lod_for_size(range2.end - range2.start));

            // The total size of `range2`
            let size2 = size2.unwrap_or_else(|| model.line_total_size(range2.clone(), lod2 > 0));

            let former_len = self.line_grs.offset_len().index;

            // Insert `range2` as a new line group
            let line_gr = LineGr {
                num_lines: range2.end - range2.start,
                size: size2,
            };

            if range2.start == self.line_grs.offset_len().index {
                self.line_grs.push_back(line_gr);
            } else {
                self.line_grs
                    .insert_before(line_gr, FirstAfter(by_key(LineOff::index, range2.start)))
                    .unwrap();
            }

            if lod2 > lod {
                // Create a higher-LOD group containing `range2`
                let lod_gr_start = self.lod_grs[lod_gr_i].index;
                let lod_gr_end = if let Some(gr) = self.lod_grs.get(lod_gr_i + 1) {
                    gr.index
                } else {
                    former_len
                };

                debug_assert!(range2.start >= lod_gr_start);
                debug_assert!(range2.start < lod_gr_end);

                if range2.start == lod_gr_start {
                    self.lod_grs[lod_gr_i2].lod = lod2;
                } else {
                    lod_gr_i2 += 1;
                    self.lod_grs.insert(
                        lod_gr_i2,
                        LodGr {
                            lod: lod2,
                            index: range2.start,
                        },
                    );
                }

                if range2.start < lod_gr_end {
                    lod_gr_i2 += 1;
                    self.lod_grs.insert(
                        lod_gr_i2,
                        LodGr {
                            lod,
                            index: range2.end,
                        },
                    );
                }
            }
        }

        // Update the following LOD groups' starting indices
        for lod_gr in self.lod_grs[lod_gr_i2 + 1..].iter_mut() {
            lod_gr.index += num_lines;
        }
    }

    /// Synchronize the structure after lines are removed from the underlying
    /// model (`LinesetModel`).
    pub fn remove(&mut self, model: &dyn LinesetModel, range: Range<Index>) {
        unimplemented!()
    }

    /// Synchronize the structure after lines are resized.
    pub fn recalculate_size(&mut self, model: &dyn LinesetModel, range: Range<Index>) {
        unimplemented!()
    }

    /// Reorganize LOD groups.
    pub fn regroup(&mut self, model: &dyn LinesetModel) {
        // TODO: Get the visible portion from somewhere
        unimplemented!()
    }

    /// Validate the integrity of the structure.
    #[cfg(test)]
    fn validate(&self) {
        assert_eq!(self.lod_grs.is_empty(), self.line_grs.is_empty());
        if self.lod_grs.is_empty() {
            return;
        }

        use rope::{range_by_key, Edge::Floor};

        assert_eq!(self.lod_grs[0].index, 0);
        for i in 0..self.lod_grs.len() {
            let lod_gr = self.lod_grs[i];
            let start = lod_gr.index;
            let end = if let Some(gr) = self.lod_grs.get(i + 1) {
                gr.index
            } else {
                self.line_grs.offset_len().index
            };
            assert!(
                start < end,
                "lod_grs[{}].index ({}) < end ({})",
                i,
                start,
                end
            );

            let (iter, range) = self
                .line_grs
                .range(range_by_key(LineOff::index, Floor(start)..Floor(end)));

            // LOD groups must completely contain line groups
            assert_eq!(range.start.index, start);
            assert_eq!(range.end.index, end);

            let size_range = lod_size_range(lod_gr.lod);

            let mut iter = iter.peekable();
            while let Some(line_gr) = iter.next() {
                let is_last = iter.peek().is_none();

                assert!(
                    line_gr.num_lines <= *size_range.end(),
                    "{} <= {}",
                    line_gr.num_lines,
                    size_range.end()
                );

                if is_last {
                    assert!(line_gr.num_lines >= 1, "{} >= 1", line_gr.num_lines)
                } else {
                    assert!(
                        line_gr.num_lines >= *size_range.start(),
                        "{} >= {}",
                        line_gr.num_lines,
                        size_range.start()
                    )
                }
            }
        }
    }

    // TODO: query
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lod_size_range() {
        assert_eq!(lod_size_range(0), 1..=1);
        assert_eq!(lod_size_range(1), 1..=2);
        assert_eq!(lod_size_range(2), 2..=4);
    }

    #[test]
    fn test_min_lod_for_size() {
        for i in 1..100 {
            let lod = min_lod_for_size(i);
            assert_eq!(lod_size_range(lod).contains(&i), true);
            if lod > 0 {
                assert_eq!(lod_size_range(lod - 1).contains(&i), false);
            }
        }
    }

    struct TestModel;

    impl TestModel {
        fn pos(&self, i: Index) -> Size {
            let i = i as f64;
            (i.sin() * 10.0 + i * 15.0) as Size
        }
    }

    impl LinesetModel for TestModel {
        fn line_total_size(&self, range: Range<Index>, _approx: bool) -> Size {
            self.pos(range.end) - self.pos(range.start)
        }
    }

    #[test]
    fn insert_to_empty() {
        for i in 0..16 {
            let mut lineset = Lineset::new();
            lineset.validate();

            lineset.insert(&TestModel, 0..i);
            dbg!(&lineset);
            lineset.validate();
        }
    }

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
    fn insert_to_non_empty() {
        let mut rng = Xorshift32(0xdeadbeef);

        for _ in 0..100 {
            rng.next();
        }

        for lod in [0, 2].iter().flat_map(|&i| std::iter::repeat(i).take(4)) {
            dbg!(lod);

            let mut lineset = Lineset::new();

            // Prepare the initial state
            let size_range = lod_size_range(lod);
            for _ in 0..4 {
                lineset.lod_grs.push(LodGr {
                    index: lineset.line_grs.offset_len().index,
                    lod,
                });

                let num_line_grs = rng.next_range(0..3);
                for _ in 0..num_line_grs {
                    let line_gr_len = rng
                        .next_range(*size_range.start() as u32..*size_range.end() as u32 + 1)
                        as _;
                    lineset.line_grs.push_back(LineGr {
                        num_lines: line_gr_len,
                        size: 1,
                    });
                }

                let line_gr_len = rng.next_range(1..*size_range.end() as u32 + 1) as _;
                lineset.line_grs.push_back(LineGr {
                    num_lines: line_gr_len,
                    size: 1,
                });
            }

            dbg!(&lineset);
            lineset.validate();

            // Try insertion
            for pos in 0..=lineset.line_grs.offset_len().index {
                for &count in &[1, 2, 3, 4, 10] {
                    dbg!(pos..pos + count);
                    let mut lineset = lineset.clone();
                    lineset.insert(&TestModel, pos..pos + count);
                    dbg!(&lineset);
                    lineset.validate();
                }
            }
        }
    }
}
