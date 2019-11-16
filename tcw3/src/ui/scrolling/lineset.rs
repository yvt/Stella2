#![allow(clippy::range_plus_one)] // rust-lang/rust-clippy#3307
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::len_zero)] // rust-lang/rust-clippy#3807

use derive_more::{Add, AddAssign, Neg};
use eager_peekable::{EagerPeekable, EagerPeekableExt};
use rope::{self, Rope};
use std::{
    cmp::{max, min, Ordering},
    collections::BinaryHeap,
    ops::{Range, RangeInclusive},
};

mod debug;
mod multiset;

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
#[derive(Clone)]
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

/// The displacement handler.
///
/// The line coordinates start at 0, irregardless of the scroll position. When
/// lines are inserted, removed, or resized, subsequent lines are moved to
/// accommodate to these changes. The client should specify whether each
/// viewport should follow or not and adjust them appropriately by implementing
/// `DispCb`.
///
/// This trait is used only for resizing. For insertion and removal, the client
/// should take an appropriate action based on the return value of their
/// respective methods.
pub trait DispCb {
    /// Called when line groups are resized.
    fn line_resized(&mut self, _range: Range<Index>, _old_pos: Range<Size>, _new_pos: Range<Size>) {
    }
}

impl DispCb for () {}

impl<T: FnMut(Range<Index>, Range<Size>, Range<Size>)> DispCb for T {
    fn line_resized(&mut self, range: Range<Index>, old_pos: Range<Size>, new_pos: Range<Size>) {
        self(range, old_pos, new_pos);
    }
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

    fn pos(&self) -> Size {
        self.pos
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
#[derive(Debug, Clone, Copy, PartialEq)]
struct LodGr {
    index: Index,
    lod: u8,
}

/// An impossible LOD value.
const BAD_LOD: u8 = 255;

/// Get the valid line group size range for the specified LOD.
fn lod_size_range(lod: u8) -> RangeInclusive<Index> {
    let shift1 = lod as u32 - (lod > 0) as u32; // max(lod - 1, 0)
    let shift2 = lod as u32;
    (1 << shift1)..=(1 << shift2)
}

/// Get a flag indicating whether line group sizes can use approximation for
/// the specified LOD level or not.
///
/// The returned flag is intended to be used as a parameter value of
/// `LinesetModel::line_total_size`.
fn lod_approx(lod: u8) -> bool {
    lod > 0
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

/// Divide a size into three. This function ensures that the total size remains
/// unchanged.
fn divide_size3(size: Size, ratio: [Size; 3]) -> [Size; 3] {
    let sum = ratio[0] as f64 + ratio[1] as f64 + ratio[2] as f64;
    let mut size1 = (size as f64 * ratio[0] as f64 / sum + 0.5) as Size;
    let mut size2 = (size as f64 * ratio[1] as f64 / sum + 0.5) as Size;
    let mut size3 = size - size1 - size2;

    while size3 < 0 {
        if size1 > 0 {
            size1 -= 1;
            size3 += 1;
        } else if size2 > 0 {
            size2 -= 1;
            size3 += 1;
        } else {
            break;
        }
    }

    debug_assert!(
        size1 >= 0 && size2 >= 0 && size3 >= 0,
        "{:?}",
        (size, ratio, size1, size2, size3)
    );

    [size1, size2, size3]
}

impl Lineset {
    pub fn new() -> Self {
        Self {
            line_grs: Rope::new(),
            lod_grs: Vec::new(),
        }
    }

    /// Get the number of lines in a lineset.
    pub fn num_lines(&self) -> Index {
        self.line_grs.offset_len().index
    }

    /// Get the total size of lines in a lineset.
    pub fn total_size(&self) -> Size {
        self.line_grs.offset_len().pos
    }

    /// Synchronize the structure after new lines are inserted to the underlying
    /// model (`LinesetModel`).
    ///
    /// The time complexity of this operation is logarithmic, provided that
    /// `regroup` is called after each operation.
    ///
    /// Returns an approximate line coordinate range representing the location
    /// of the new lines, or `None` if no lines are inserted.
    pub fn insert(&mut self, model: &dyn LinesetModel, range: Range<Index>) -> Option<Range<Size>> {
        if range.end <= range.start {
            return None;
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
            let size = model.line_total_size(range, lod_approx(lod));
            let at = self.line_grs.offset_len().pos;
            self.line_grs.push_back(LineGr { num_lines, size });
            return Some(at..at + size);
        }

        // Find the LOD group the new lines belong to
        let lod_gr_i = match self.lod_grs.binary_search_by_key(&range.start, |g| g.index) {
            Ok(i) => i,
            Err(i) => i - 1,
        };

        let lod = self.lod_grs[lod_gr_i].lod;
        let lod_size_range = lod_size_range(lod);

        // Find the line group the new lines are inserted to
        use rope::{by_key, range_by_key, Edge::Floor, One::FirstAfter};
        let (line_gr, line_gr_off) = {
            let (mut iter, range) = self
                .line_grs
                .range(range_by_key(LineOff::index, Floor(range.start)..));
            (*iter.nth(0).unwrap(), range.start)
        };

        // Endpoints of the line group (pre-insertion)
        let line_gr_start = line_gr_off.index;
        let line_gr_end = line_gr_start + line_gr.num_lines;

        let next;

        let (inserted_pos, mut inserted_size);

        // TODO: Maybe delegate this complexity to `regroup`?

        if range.start != line_gr_start || num_lines < *lod_size_range.start() {
            debug_assert!(lod > 0);

            // The total size of the new lines
            let size = model.line_total_size(range.clone(), lod_approx(lod));

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

                inserted_pos = divide_size(
                    line_gr.size,
                    [
                        model.line_total_size(line_gr_start..range.start, lod_approx(lod)),
                        model.line_total_size(range.end..line_gr_end + num_lines, lod_approx(lod)),
                    ],
                )[0] + line_gr_off.pos;
                inserted_size = size;
            } else if *lod_size_range.end() * 2 - line_gr.num_lines >= num_lines {
                // Insert the new lines to the existing line group, and then
                // divide it into two to satisfy the invariant.
                let new_gr_num_lines = line_gr.num_lines + num_lines;
                let new_gr_mid = line_gr_start + (new_gr_num_lines >> 1);

                let halve_sizes_new;
                if range.start > new_gr_mid {
                    // Divide `line_gr` at `new_gr_mid`.
                    let halve_sizes_old = divide_size3(
                        line_gr.size,
                        [
                            model.line_total_size(line_gr_start..new_gr_mid, lod_approx(lod)),
                            model.line_total_size(new_gr_mid..range.start, lod_approx(lod)),
                            model.line_total_size(
                                range.end..line_gr_end + num_lines,
                                lod_approx(lod),
                            ),
                        ],
                    );

                    // The new lines belongs to the second half
                    halve_sizes_new = [
                        halve_sizes_old[0],
                        halve_sizes_old[1] + size + halve_sizes_old[2],
                    ];

                    inserted_pos = halve_sizes_old[0] + halve_sizes_old[1] + line_gr_off.pos;
                    inserted_size = size;
                } else if range.end > new_gr_mid {
                    // Divide `line_gr` at `new_gr_mid`.
                    let halve_sizes_old = divide_size(
                        line_gr.size,
                        [
                            model.line_total_size(line_gr_start..range.start, lod_approx(lod)),
                            model.line_total_size(
                                range.end..line_gr_end + num_lines,
                                lod_approx(lod),
                            ),
                        ],
                    );

                    // The new lines are split into both halves
                    let size2 = [
                        model.line_total_size(range.start..new_gr_mid, lod_approx(lod)),
                        model.line_total_size(new_gr_mid..range.end, lod_approx(lod)),
                    ];
                    halve_sizes_new =
                        [halve_sizes_old[0] + size2[0], halve_sizes_old[1] + size2[1]];

                    inserted_pos = halve_sizes_old[0] + line_gr_off.pos;
                    inserted_size = size2[0] + size2[1];
                } else {
                    // Divide `line_gr` at `new_gr_mid`.
                    let halve_sizes_old = divide_size3(
                        line_gr.size,
                        [
                            model.line_total_size(line_gr_start..range.start, lod_approx(lod)),
                            model.line_total_size(range.end..new_gr_mid, lod_approx(lod)),
                            model.line_total_size(
                                new_gr_mid..line_gr_end + num_lines,
                                lod_approx(lod),
                            ),
                        ],
                    );

                    // The new lines belongs to the first half
                    halve_sizes_new = [
                        halve_sizes_old[0] + size + halve_sizes_old[1],
                        halve_sizes_old[2],
                    ];

                    inserted_pos = halve_sizes_old[0] + line_gr_off.pos;
                    inserted_size = size;
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
                        model.line_total_size(line_gr_start..range.start, lod_approx(lod)),
                        model.line_total_size(range.end..line_gr_end + num_lines, lod_approx(lod)),
                    ],
                );

                // Apply the adjustment to `halve_sizes`
                let halve_sizes_postadj = [
                    halve_sizes[0]
                        + model.line_total_size(range.start..new_range.start, lod_approx(lod)),
                    halve_sizes[1]
                        + model.line_total_size(new_range.end..range.end, lod_approx(lod)),
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
                let new_size = model.line_total_size(new_range.clone(), lod_approx(lod));

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

                inserted_pos = halve_sizes[0] + line_gr_off.pos;
                inserted_size = halve_sizes_postadj[0] + halve_sizes_postadj[1]
                    - halve_sizes[0]
                    - halve_sizes[1];
            }
        } else {
            next = Some((range, None));

            inserted_pos = line_gr_off.pos;
            inserted_size = 0;
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
            let size2 =
                size2.unwrap_or_else(|| model.line_total_size(range2.clone(), lod_approx(lod2)));

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
                let lod_gr_end = lod_gr_end(former_len, &self.lod_grs, lod_gr_i);

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

            inserted_size += size2;
        }

        // Update the following LOD groups' starting indices
        for lod_gr in self.lod_grs[lod_gr_i2 + 1..].iter_mut() {
            lod_gr.index += num_lines;
        }

        Some(inserted_pos..inserted_pos + inserted_size)
    }

    /// Synchronize the structure *before* lines are removed from the underlying
    /// model (`LinesetModel`).
    ///
    /// Returns an approximate line coordinate range representing the original
    /// location of the removed lines, or `None` if no lines are removed.
    pub fn remove(&mut self, model: &dyn LinesetModel, range: Range<Index>) -> Option<Range<Size>> {
        if range.end <= range.start {
            return None;
        }
        assert!(range.end <= self.line_grs.offset_len().index);
        assert!(range.start >= 0);

        use rope::{
            by_key, range_by_key,
            Edge::{Ceil, Floor},
            One::FirstAfter,
        };

        let num_lines = range.end - range.start;

        // Find the LOD group `range.start` belong to
        let lod_gr_i1 = match self.lod_grs.binary_search_by_key(&range.start, |g| g.index) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let lod1 = self.lod_grs[lod_gr_i1].lod;
        let lod_size_range1 = lod_size_range(lod1);

        // Find line groups overlapping with `range`
        let (mut line_gr_iter, line_gr_range) = self.line_grs.range(range_by_key(
            |off: &LineOff| off.index,
            Floor(range.start)..Ceil(range.end),
        ));

        debug_assert!(line_gr_range.start.index <= range.start);
        debug_assert!(line_gr_range.end.index >= range.end);

        // Line groups of respective endpoints. `line_gr2` is `None` iff the
        // range contains only one line group.
        //
        //     Line grs:        [gr1               ] [          ]
        //     line_gr_range:   [                  ]
        //     range:               [           ]
        //
        //     Line grs:        [gr1    ] [        ] [gr2       ]
        //     line_gr_range:   [                               ]
        //     range:               [                    ]
        //
        //     Line grs:        [gr1    ] [        ] [gr2       ]
        //     line_gr_range:   [                               ]
        //     range:           [                               ]
        //
        let line_gr1: LineGr = line_gr_iter.next().cloned().unwrap();
        let line_gr2: Option<LineGr> = line_gr_iter.next_back().cloned();
        drop(line_gr_iter);

        if line_gr2.is_none()
            && (range.start != line_gr_range.start.index || range.end != line_gr_range.end.index)
        {
            // - `range` overlaps with exactly one line group.
            // -  And, `range` partially (not fully) overlaps the line group.
            //
            //     Line grs:        [gr1                            ]
            //     line_gr_range:   [                               ]
            //     range:               [                    ]
            //

            // The end of this LOD group (`lod_gr_i1`)
            let lod_gr_end = lod_gr_end(self.line_grs.offset_len().index, &self.lod_grs, lod_gr_i1);

            debug_assert!(lod1 > 0);

            let remaining_num_lines = line_gr1.num_lines - num_lines;
            if remaining_num_lines < *lod_size_range1.start()
                && line_gr_range.end.index < lod_gr_end
            {
                // It'll violate the size invariant unless it's the last
                // line group in a LOD group. So make it the last group
                // (temporarily).
                self.lod_grs.insert(
                    lod_gr_i1 + 1,
                    LodGr {
                        index: line_gr_range.end.index,
                        lod: lod1,
                    },
                );
            }

            // Estimate the size of the removed part
            let size1 =
                model.line_total_size(line_gr_range.start.index..range.start, lod_approx(lod1));
            let size2 = model.line_total_size(range.clone(), lod_approx(lod1));
            let size3 = model.line_total_size(range.end..line_gr_range.end.index, lod_approx(lod1));
            let [size1, size2, size3] = divide_size3(line_gr1.size, [size1, size2, size3]);
            let remaining_size = size1 + size3;

            // Remove `range` from the line group
            self.line_grs
                .update_with(
                    FirstAfter(by_key(LineOff::index, range.start)),
                    |line_gr, _| {
                        line_gr.size = remaining_size;
                        line_gr.num_lines = remaining_num_lines;
                    },
                )
                .unwrap();

            // Update the following LOD groups' starting indices
            for lod_gr in self.lod_grs[lod_gr_i1 + 1..].iter_mut() {
                lod_gr.index -= num_lines;
            }

            return Some(line_gr_range.start.pos + size1..line_gr_range.start.pos + size1 + size2);
        }

        // Find the LOD group `range.end` belong to
        let lod_gr_i2 = match self.lod_grs.binary_search_by_key(&range.end, |g| g.index) {
            Ok(i) => i - 1,
            Err(i) => i - 1,
        };
        let lod2 = self.lod_grs[lod_gr_i2].lod;
        let lod_size_range2 = lod_size_range(lod2);

        // The range of the LOD group `lod_gr_i2`
        let lod_gr2_start = self.lod_grs[lod_gr_i2].index;
        let lod_gr2_end = if let Some(lod_gr) = self.lod_grs.get(lod_gr_i2 + 1) {
            lod_gr.index
        } else {
            self.line_grs.offset_len().index
        };

        debug_assert!(lod_gr2_start < range.end);
        debug_assert!(lod_gr2_end >= range.end);

        let mut removed_size = 0;

        // The first LOD group `lod_gr` such that `lod_g.index >= bulk_delete_end`
        let lod_bulk_delete_end;

        // Process the ending point first to minimize the number of invalidated
        // indices.
        if range.end < line_gr_range.end.index {
            // `range.end` is in the middle of `line_gr2`. `line_gr2` remains,
            // but some of its lines in its front are removed.
            let line_gr2 = line_gr2.unwrap();

            debug_assert!(lod2 > 0);

            let line_gr2_start = line_gr_range.end.index - line_gr2.num_lines;
            let line_gr2_end = line_gr_range.end.index;

            let remaining_num_lines = line_gr2_end - range.end;
            if remaining_num_lines < *lod_size_range2.start()
                && line_gr_range.end.index < lod_gr2_end
            {
                // It'll violate the size invariant unless it's the last
                // line group in a LOD group. So make it the last group
                // (temporarily).
                self.lod_grs.insert(
                    lod_gr_i2 + 1,
                    LodGr {
                        index: line_gr_range.end.index,
                        lod: lod2,
                    },
                );
            }

            // Estimate the size of the removed part
            let size1 = model.line_total_size(line_gr2_start..range.end, lod_approx(lod2));
            let size2 = model.line_total_size(range.end..line_gr2_end, lod_approx(lod2));
            let [cut_size, remaining_size] = divide_size(line_gr2.size, [size1, size2]);

            // Remove a partial range from `line_gr2`
            self.line_grs
                .update_with(
                    FirstAfter(by_key(LineOff::index, range.end)),
                    |line_gr, _| {
                        line_gr.size = remaining_size;
                        line_gr.num_lines = remaining_num_lines;
                    },
                )
                .unwrap();

            removed_size += cut_size;

            if lod_gr2_start < line_gr2_start {
                // Split the LOD group at `range.end` because the portion
                // before `range.start` might belong to a different LOD group.
                //
                //     Line grs:     [      ] [     ] [      ]
                //     LOD grs:      [1       [2
                //       (after):    [1       [2           [2
                //       (post-bulk-deletion):
                //                   [1                    [2
                //     range:          [                  ]
                //
                self.lod_grs.insert(
                    lod_gr_i2 + 1,
                    LodGr {
                        index: range.end,
                        lod: lod2,
                    },
                );
                lod_bulk_delete_end = lod_gr_i2 + 1;
            } else {
                //
                //     Line grs:     [      ] [     ] [      ]
                //     LOD grs:      [1               [2
                //       (after):    [1                    [2
                //       (post-bulk-deletion):
                //                   [1                    [2
                //     range:           [                 ]
                //
                debug_assert_eq!(lod_gr2_start, line_gr2_start);
                self.lod_grs[lod_gr_i2].index = range.end;
                lod_bulk_delete_end = lod_gr_i2;
            }
        } else {
            // `range.end` is right after `line_gr2.unwrap_or(line_gr1)`.
            if lod_gr2_end > range.end {
                // Split the LOD group after `line_gr2` because `line_gr1` might
                // belong to a different LOD group.
                //
                //     Line grs:     [      ] [     ] [      ]
                //     LOD grs:      [1       [2
                //       (after):    [1       [2      [2
                //       (post-bulk-deletion):
                //                   [1               [2
                //     range:           [           ]
                //
                self.lod_grs.insert(
                    lod_gr_i2 + 1,
                    LodGr {
                        index: range.end,
                        lod: lod2,
                    },
                )
            } else {
                //
                //     Line grs:     [      ] [     ] [      ]
                //     LOD grs:      [1       [2      [3
                //       (post-bulk-deletion):
                //                   [1               [3
                //     range:           [           ]
                //
                debug_assert_eq!(lod_gr2_end, range.end);
            }
            lod_bulk_delete_end = lod_gr_i2 + 1;
        }

        // The range of the LOD group `lod_gr_i1`
        let lod_gr1_start = self.lod_grs[lod_gr_i1].index;

        debug_assert!(lod_gr1_start <= range.start);

        // Remove full line groups (we call this step "bulk removal")
        let bulk_delete_start = if range.start > line_gr_range.start.index {
            line_gr_range.start.index + line_gr1.num_lines
        } else {
            line_gr_range.start.index
        };
        let bulk_delete_end = if range.end < line_gr_range.end.index {
            line_gr_range.end.index - line_gr2.unwrap().num_lines
        } else {
            line_gr_range.end.index
        };

        let mut num_bulk_deleted_lines = bulk_delete_end - bulk_delete_start;

        while num_bulk_deleted_lines > 0 {
            let (line_gr, _) = self
                .line_grs
                .remove(FirstAfter(by_key(LineOff::index, bulk_delete_start)))
                .unwrap();
            num_bulk_deleted_lines -= line_gr.num_lines;
            debug_assert!(num_bulk_deleted_lines >= 0);

            removed_size += line_gr.size;
        }

        // Delete starting points of LOD groups in
        // `[bulk_delete_start, bulk_delete_end)`
        debug_assert!(bulk_delete_start >= lod_gr1_start);
        let lod_bulk_delete_start = if lod_gr1_start == bulk_delete_start {
            lod_gr_i1
        } else {
            lod_gr_i1 + 1
        };
        vec_remove_range(
            &mut self.lod_grs,
            lod_bulk_delete_start..lod_bulk_delete_end,
        );

        let removed_pos;

        if range.start > line_gr_range.start.index {
            // `range.start` is in the middle of `line_gr1`.  `line_gr1` remains,
            // but some of its lines in its front are removed.
            debug_assert!(lod1 > 0);

            let line_gr1_start = line_gr_range.start.index;
            let line_gr1_end = line_gr_range.start.index + line_gr1.num_lines;

            let remaining_num_lines = range.start - line_gr1_start;
            // It's okay for `remaining_num_lines` to go under
            // `lod_size_range1.start()` because we made sure that `line_gr1`
            // was the last line group in the LOD group.
            debug_assert!(
                if let Some(lod_gr) = self.lod_grs.get(lod_bulk_delete_start) {
                    lod_gr.index == range.end
                } else {
                    true
                }
            );

            // Estimate the size of the removed part
            let size1 = model.line_total_size(line_gr1_start..range.start, lod_approx(lod1));
            let size2 = model.line_total_size(range.start..line_gr1_end, lod_approx(lod1));
            let [remaining_size, cut_size] = divide_size(line_gr1.size, [size1, size2]);

            // Remove a partial range from `line_gr1`
            self.line_grs
                .update_with(
                    FirstAfter(by_key(LineOff::index, range.start)),
                    |line_gr, _| {
                        line_gr.size = remaining_size;
                        line_gr.num_lines = remaining_num_lines;
                    },
                )
                .unwrap();

            removed_pos = line_gr_range.start.pos + remaining_size;
            removed_size += cut_size;
        } else {
            removed_pos = line_gr_range.start.pos;
        }

        // Adjust the starting point of the LOD groups following `range`
        for lod_gr in self.lod_grs[lod_bulk_delete_start..].iter_mut() {
            lod_gr.index -= num_lines;
        }

        Some(removed_pos..removed_pos + removed_size)
    }

    /// Synchronize the structure after lines are resized.
    ///
    /// This method recalculates the sizes of line groups overlapping with
    /// `range`. If their sizes change, it's reported back via `disp_cb`
    /// so that the caller can make amendments to viewport positions.
    ///
    /// Non-zero LOD levels are ignored if `skip_approx == true`.
    ///
    /// TODO: What is the purpose of `skip_approx`?
    pub fn recalculate_size(
        &mut self,
        model: &dyn LinesetModel,
        range: Range<Index>,
        skip_approx: bool,
        disp_cb: &mut dyn DispCb,
    ) {
        use rope::{by_key, One::FirstAfter};

        let num_lines = self.line_grs.offset_len().index;
        let range = max(range.start, 0)..min(range.end, num_lines);
        if range.start >= range.end {
            return;
        }

        let lod_gr1_i = match self.lod_grs.binary_search_by_key(&range.start, |g| g.index) {
            Ok(i) => i,
            Err(i) => i - 1,
        };

        for (lod_gr, lod_gr_end) in iter_lod_gr_with_end(num_lines, &self.lod_grs[lod_gr1_i..])
            .take_while(|lod_gr| lod_gr.0.index < range.end)
        {
            let approx = lod_approx(lod_gr.lod);

            if skip_approx && approx {
                continue;
            }

            let mut next = max(lod_gr.index, range.start);

            while next < min(lod_gr_end, range.end) {
                // TODO: This is utterly inefficient (O(n log n))
                //       Maybe add something like `update_range_with` to `Rope`?
                self.line_grs.update_with(
                    FirstAfter(by_key(LineOff::index, next)),
                    |line_gr, line_off| {
                        let gr_range = line_off.index..line_off.index + line_gr.num_lines;
                        let new_size = model.line_total_size(gr_range.clone(), approx);
                        if new_size != line_gr.size {
                            disp_cb.line_resized(
                                gr_range.clone(),
                                line_off.pos..line_off.pos + line_gr.size,
                                line_off.pos..line_off.pos + new_size,
                            );
                            line_gr.size = new_size;
                        }
                        next = gr_range.end;
                    },
                );
            }
        }
    }

    /// Reorganize LOD groups.
    ///
    /// The regroup operation subdivides and/or decimates LOD groups to make
    /// the region specified by `viewports` more detailed, while others less
    /// detailed. It tries to preserve the total size of a set of lines, except
    /// when it subdivides a line group producing a LOD-0 line group. LOD-0 line
    /// groups have accurate sizes (calculated by specifying `false` as a
    /// `approx` parameter), so their sizes must be updated when they are
    /// created from LOD-1 line groups. This displaces all subsequent lines,
    /// thus some viewports have to be moved as well. To make this possible,
    /// this method calls `disp_cb.line_resized` whenever line groups are
    /// resized.
    ///
    /// A lineset is said to be *well-grouped* for a set of line coordinates `P`
    /// after being `regroup`ed for a set of viewports covering `P`. If a
    /// lineset is well-grouped for `P₁` before being `regroup`ed for `P₂`, the
    /// regrouping process does not touch any lines touching `P₁ ∩ P₂`.
    ///
    /// Note that regrouping might shrink some line groups, even those
    /// previously overlapping with `viewports`.
    pub fn regroup(
        &mut self,
        model: &dyn LinesetModel,
        viewports: &[Range<Size>],
        disp_cb: &mut dyn DispCb,
    ) {
        use rope::{
            by_key, range_by_key,
            Edge::{Ceil, Floor},
            One::{FirstAfter, LastBefore},
        };

        let num_lines = self.line_grs.offset_len().index;

        if num_lines == 0 {
            return;
        }

        // Split line groups to lower their LOD levels (up to LOD 1)
        // -----------------------------------------------------------------
        // The goal of this step is to reduce the conservativeness of the
        // conversion from `Range<Size>` to `Range<Index>`.
        // Do not go further than LOD 1 as doing so would resize lines.
        // (`viewports` would be invalidated if lines were resized.)
        let mut lod_grs2 = Vec::with_capacity(self.lod_grs.len() * 2);

        for vp_by_pos in viewports.iter() {
            // Convert `Range<Size>` to `Range<Index>`. This might be
            // overconservative if they cross large line groups.
            let vp_by_idx = {
                let (_, range) = self.line_grs.range(range_by_key(
                    LineOff::pos,
                    Floor(vp_by_pos.start)..Ceil(vp_by_pos.end),
                ));

                range.start.index..range.end.index
            };

            let lod_gr1_i = match self
                .lod_grs
                .binary_search_by_key(&vp_by_idx.start, |g| g.index)
            {
                Ok(i) => i,
                Err(i) => {
                    if vp_by_idx.start == self.num_lines() {
                        i
                    } else {
                        i - 1
                    }
                }
            };
            let lod_gr2_i = match self
                .lod_grs
                .binary_search_by_key(&vp_by_idx.end, |g| g.index)
            {
                Ok(i) => i,
                Err(i) => i,
            };

            // Do we have to process these LOD groups?
            let skip = self.lod_grs[lod_gr1_i..lod_gr2_i]
                .iter()
                .all(|gr| gr.lod <= 1);
            if skip {
                continue;
            }

            lod_grs2.extend(self.lod_grs[..lod_gr1_i].iter().cloned());
            for i in lod_gr1_i..lod_gr2_i {
                let lod_gr_start = self.lod_grs[i].index;
                let lod_gr_end = lod_gr_end(num_lines, &self.lod_grs, i);

                let vp_range = max(vp_by_idx.start, lod_gr_start)..min(vp_by_idx.end, lod_gr_end);

                check_presplit(
                    &mut lod_grs2,
                    self.lod_grs[i].lod,
                    lod_gr_start..lod_gr_end,
                    vp_range,
                    vp_by_pos.clone(),
                    &mut self.line_grs,
                    model,
                );
            }
            lod_grs2.extend(self.lod_grs[lod_gr2_i..].iter().cloned());

            std::mem::swap(&mut self.lod_grs, &mut lod_grs2);
            lod_grs2.clear();
        }

        // Generates a LOD-`lod` group covering `range`. Before doing so,
        // subdivide a portion of the LOD-`lod` group that includes
        // `vp_pos_range` and recursively call `check_presplit` to generate a
        // lower-level LOD group for that portion.
        //
        // `vp_pos_range` is the desired portion to be subdivided, `vp_range`
        // is its approximation, aligned to line group boundaries. After the
        // function call, we can get more accurate `vp_range`.
        //
        // `vp_range` must be a non-strict subset of `range`.
        //
        //     (before)
        //                               vp_pos_range (by line coordinates)
        //                                 vvvvvvvvvvvvvvvvvvv
        //     line grs: [        ] [        ] [        ] [        ] [        ]
        //                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        //                            vp_range (by indices)
        //     LOD grs:  [ 2                                                  ]
        //               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        //                range (by indices)
        //
        //     (in progress - after `line_gr_lower_lod_incl`)
        //
        //                                 vvvvvvvvvvvvvvvvvvv
        //     line grs: [        ] [   ] [  ] [   ] [  ] [   ] [  ] [        ]
        //                                ^^^^^^^^^^^^^^^^^^^^^
        //                                 vp_range2
        //     LOD grs:  [ 2      ] [ 1                            ] [ 2      ]
        //
        fn check_presplit(
            out_lod_grs: &mut Vec<LodGr>,
            lod: u8,
            range: Range<Index>,
            vp_range: Range<Size>,
            vp_pos_range: Range<Size>,
            line_grs: &mut Rope<LineGr, LineOff>,
            model: &dyn LinesetModel,
        ) {
            debug_assert!(vp_range.start >= range.start);
            debug_assert!(vp_range.end <= range.end);

            let noop =
                // Sufficiently fine-grained?
                lod <= 1 ||
                // If `vp_range` is empty, that means `vp_pos_range` is also
                // empty and exactly points a line group boundary. Thus
                // subdivision is not necessary.
                vp_range.start == vp_range.end;

            if noop || vp_range.start > range.start {
                out_lod_grs.push(LodGr {
                    index: range.start,
                    lod,
                });
            }

            if noop {
                return;
            }

            // Subdivide the line groups covering `vp_range` (exactly)
            let SubdivResult {
                subdived_range: sub_range,
                ..
            } = line_gr_subdiv_incl(line_grs, lod - 1, vp_range.clone(), model);
            debug_assert_eq!(sub_range, vp_range);

            // Now we can get a more accurate `vp_range`
            let vp_range2 = {
                let (_, range) = line_grs.range(range_by_key(
                    LineOff::pos,
                    Floor(vp_pos_range.start)..Ceil(vp_pos_range.end),
                ));

                range.start.index..range.end.index
            };
            let vp_range2 = max(vp_range2.start, vp_range.start)..min(vp_range2.end, vp_range.end);

            // Recursively process the portion `vp_range`
            check_presplit(
                out_lod_grs,
                lod - 1,
                vp_range.clone(), // `range` (where a LOD group is generated)
                vp_range2,        // `vp_range` (the portion we want to be subdivided)
                vp_pos_range,
                line_grs,
                model,
            );

            if vp_range.end < range.end {
                out_lod_grs.push(LodGr {
                    index: vp_range.end,
                    lod,
                });
            }
        }

        // Convert `Range<Size>`s to `Range<Index>`s again, based on the new
        // subdivision.
        let viewports_by_idx = viewports.iter().map(|pos_range| {
            let (_, range) = self.line_grs.range(range_by_key(
                LineOff::pos,
                Floor(pos_range.start)..Ceil(pos_range.end),
            ));

            range.start.index..range.end.index
        });

        // Create the goal LOD group list
        // -----------------------------------------------------------------
        // `O(num_lod_grs * log(viewports.len()))`
        let goal_lod_grs = lod_grs_from_vps(num_lines, self.lod_grs.len() * 2, viewports_by_idx);

        // Subdivide line groups to lower their LOD levels until the goal is reached
        // -----------------------------------------------------------------
        let mut goal_lod_gr_it = iter_lod_gr_with_end(num_lines, &goal_lod_grs).eager_peekable();

        // For each existing LOD group...
        for (lod_gr, lod_gr_end) in iter_lod_gr_with_end(num_lines, &self.lod_grs) {
            check_subdiv_decim(
                &mut lod_grs2,
                lod_gr.lod,
                lod_gr.index..lod_gr_end,
                &mut goal_lod_gr_it,
                &mut self.line_grs,
                &mut Subdiv { model, disp_cb },
            );
        }

        struct Subdiv<'a> {
            model: &'a dyn LinesetModel,
            disp_cb: &'a mut dyn DispCb,
        }

        impl SubdivDecim for Subdiv<'_> {
            fn should_process_lod(&self, lod: u8, goal: u8) -> bool {
                lod > goal
            }
            fn process_range(
                &mut self,
                lod: u8,
                range: Range<Index>,
                line_grs: &mut Rope<LineGr, LineOff>,
            ) -> Option<(Range<Index>, u8)> {
                let SubdivResult {
                    subdived_range,
                    pos,
                    old_size,
                    new_size,
                } = line_gr_subdiv_incl(line_grs, lod - 1, range.clone(), self.model);

                // This is what `incl` means
                debug_assert!(
                    subdived_range.start <= range.start && subdived_range.end >= range.end,
                    "{:?} ⊇ {:?}",
                    &subdived_range,
                    &range,
                );

                if old_size != new_size {
                    self.disp_cb.line_resized(
                        subdived_range.clone(),
                        pos..pos + old_size,
                        pos..pos + new_size,
                    );
                }

                Some((subdived_range, lod - 1))
            }
        }

        // Decimate line groups to raise their LOD levels until the goal is reached
        // -----------------------------------------------------------------
        std::mem::swap(&mut self.lod_grs, &mut lod_grs2);
        lod_grs2.clear();

        let mut goal_lod_gr_it = iter_lod_gr_with_end(num_lines, &goal_lod_grs).eager_peekable();

        // For each existing LOD group...
        for (lod_gr, lod_gr_end) in iter_lod_gr_with_end(num_lines, &self.lod_grs) {
            check_subdiv_decim(
                &mut lod_grs2,
                lod_gr.lod,
                lod_gr.index..lod_gr_end,
                &mut goal_lod_gr_it,
                &mut self.line_grs,
                &mut Decim,
            );
        }

        struct Decim;

        impl SubdivDecim for Decim {
            fn should_process_lod(&self, lod: u8, goal: u8) -> bool {
                lod < goal
            }
            fn process_range(
                &mut self,
                lod: u8,
                range: Range<Index>,
                line_grs: &mut Rope<LineGr, LineOff>,
            ) -> Option<(Range<Index>, u8)> {
                line_gr_decim_excl(line_grs, range.clone()).map(|actual_range| {
                    // This is what `excl` means
                    debug_assert!(
                        actual_range.start >= range.start && actual_range.end <= range.end,
                        "{:?} ⊆ {:?}",
                        &actual_range,
                        &range,
                    );

                    (actual_range, lod + 1)
                })
            }
        }

        // The common routine used by the previous two steps
        // -----------------------------------------------------------------

        /// Controls the behavior of `check_subdiv_decim`.
        trait SubdivDecim {
            /// Get a flag indicating whether line groups of LOD level `lod`
            /// need to be sudivided or decimated, or not.
            fn should_process_lod(&self, lod: u8, goal: u8) -> bool;
            /// Subdivide or decimate the specified portion. If the operation
            /// was performed, it returns the actual affected range and its new
            /// LOD level.
            fn process_range(
                &mut self,
                lod: u8,
                range: Range<Index>,
                line_grs: &mut Rope<LineGr, LineOff>,
            ) -> Option<(Range<Index>, u8)>;
        }

        // Generates a LOD-`lod` group covering `range`. Before doing so,
        // `goal_lod_gr_it` is examined for `range` and if it contains a
        // lower/upper-level (whichever defined by
        // `SubdivDecim::should_process_lod`) LOD group, subdivide/decimate the
        // corresponding portion of the LOD-`lod` group (using
        // `SubdivDecim::process_range`) and recursively call
        // `check_subdiv_decim` to generate a LOD group with an appropriate LOD
        // level for that portion.
        fn check_subdiv_decim(
            out_lod_grs: &mut Vec<LodGr>,
            lod: u8,
            range: Range<Index>,
            goal_lod_gr_it: &mut EagerPeekable<IterLodGrWithEnd<'_>>,
            line_grs: &mut Rope<LineGr, LineOff>,
            subdiv_decim: &mut impl SubdivDecim,
        ) {
            debug_assert!(range.start < range.end, "{:?}", range);

            while goal_lod_gr_it.peek().unwrap().1 <= range.start {
                goal_lod_gr_it.next().unwrap();
            }

            // The starting position of the next LOD-`lod` group
            let mut i = range.start;

            loop {
                let mut cur_goal_lod_gr = *goal_lod_gr_it.peek().unwrap();
                if subdiv_decim.should_process_lod(lod, cur_goal_lod_gr.0.lod) {
                    let mut sub_goal_lod_gr_it = goal_lod_gr_it.clone();

                    // A subdivided portion starts here. Search for the ending
                    // position.
                    let sub_start_unrounded = cur_goal_lod_gr.0.index;
                    let mut sub_end_unrounded = cur_goal_lod_gr.1;
                    loop {
                        // If `cur_goal_lod_gr` has a greater-or-equal (when
                        // subdividing) LOD, stop there.
                        if !subdiv_decim.should_process_lod(lod, cur_goal_lod_gr.0.lod) {
                            break;
                        }
                        sub_end_unrounded = cur_goal_lod_gr.1;
                        // <:  The cases where the loops proceeds
                        // ==: The loop'll be terminated by the next `if`. To
                        //     satisfy the postcondition, move `goal_lod_gr_it`
                        //     forward.
                        if cur_goal_lod_gr.1 <= range.end {
                            goal_lod_gr_it.next();
                        }
                        // If `cur_goal_lod_gr` fills the rest of `range`, stop
                        // at the end of `cur_goal_lod_gr`.
                        if cur_goal_lod_gr.1 >= range.end {
                            break;
                        }
                        cur_goal_lod_gr = *goal_lod_gr_it.peek().unwrap();
                    }

                    let sub_start_unrounded = max(sub_start_unrounded, i);
                    let sub_end_unrounded = min(sub_end_unrounded, range.end);

                    // Subdivide/decimate the portion
                    let result = if sub_start_unrounded >= sub_end_unrounded {
                        None
                    } else {
                        subdiv_decim.process_range(
                            lod,
                            sub_start_unrounded..sub_end_unrounded,
                            line_grs,
                        )
                    };

                    let (sub_range, new_lod) = if let Some((sub_range, new_lod)) = result {
                        (sub_range, new_lod)
                    } else {
                        // A decimate operation did not occur because the subrange
                        // was too small
                        if subdiv_decim.should_process_lod(lod, cur_goal_lod_gr.0.lod)
                            && cur_goal_lod_gr.1 >= range.end
                        {
                            break;
                        }
                        continue;
                    };

                    debug_assert!(sub_range.start >= range.start);
                    debug_assert!(sub_range.end <= range.end);
                    debug_assert!(sub_range.start < sub_range.end);
                    debug_assert!(sub_range.start >= i);

                    if sub_range.start > i {
                        out_lod_grs.push(LodGr { index: i, lod });
                    }

                    // Recursively process the portion
                    check_subdiv_decim(
                        out_lod_grs,
                        new_lod,
                        sub_range.clone(),
                        &mut sub_goal_lod_gr_it,
                        line_grs,
                        subdiv_decim,
                    );

                    i = sub_range.end;
                    if subdiv_decim.should_process_lod(lod, cur_goal_lod_gr.0.lod)
                        && cur_goal_lod_gr.1 >= range.end
                    {
                        break;
                    }
                } else {
                    if cur_goal_lod_gr.1 <= range.end {
                        goal_lod_gr_it.next();
                    }
                    if cur_goal_lod_gr.1 >= range.end {
                        break;
                    }
                }
            }

            if i < range.end {
                out_lod_grs.push(LodGr { index: i, lod });
            }
        }

        // Merge adjacent LOD groups with identical LOD levels
        // -----------------------------------------------------------------
        let mut old_i = 0;
        let mut new_i = 0;
        let mut last_lod = BAD_LOD; // impossible LOD value

        let lod_grs = &mut lod_grs2[..];

        while old_i < lod_grs.len() {
            let lod_gr = lod_grs[old_i];
            old_i += 1;

            if lod_gr.lod == last_lod {
                // Has the same LOD level as the last one - merge!
                let lod_size_range = lod_size_range(lod_gr.lod);
                let last_line_gr = *self
                    .line_grs
                    .get(LastBefore(by_key(LineOff::index, lod_gr.index)))
                    .unwrap();

                if last_line_gr.num_lines >= *lod_size_range.start() {
                    // Already has a sufficient number of lines for it to be a
                    // non-last line group in a LOD group - just remove the
                    // LOD group boundary.
                    continue;
                }

                debug_assert!(lod_gr.lod > 0);

                // Move some or all of the lines of `FirstAfter(lod_gr.index)`
                // to `last_line_gr`.
                let (merge, moved_num_lines, moved_size) = self
                    .line_grs
                    .update_with(
                        FirstAfter(by_key(LineOff::index, lod_gr.index)),
                        |first_line_gr, off| {
                            let total_num_lines = last_line_gr.num_lines + first_line_gr.num_lines;

                            if total_num_lines <= *lod_size_range.end() {
                                // Move all of the lines (deletes `first_line_gr`)
                                (true, first_line_gr.num_lines, first_line_gr.size)
                            } else {
                                // Distribte the lines evenly
                                let moved_num_lines =
                                    (first_line_gr.num_lines - last_line_gr.num_lines + 1) >> 1;

                                let i = off.index;
                                let [moved_size, remaining_size] = divide_size(
                                    first_line_gr.size,
                                    [
                                        model.line_total_size(i..i + moved_num_lines, true),
                                        model.line_total_size(
                                            i + moved_num_lines..i + first_line_gr.num_lines,
                                            true,
                                        ),
                                    ],
                                );

                                first_line_gr.num_lines -= moved_num_lines;
                                first_line_gr.size = remaining_size;

                                (false, moved_num_lines, moved_size)
                            }
                        },
                    )
                    .unwrap();

                if merge {
                    self.line_grs
                        .remove(FirstAfter(by_key(LineOff::index, lod_gr.index)))
                        .unwrap();
                }

                self.line_grs
                    .update_with(
                        LastBefore(by_key(LineOff::index, lod_gr.index)),
                        |last_line_gr, _| {
                            last_line_gr.num_lines += moved_num_lines;
                            last_line_gr.size += moved_size;
                        },
                    )
                    .unwrap();
            } else {
                last_lod = lod_gr.lod;
                lod_grs[new_i] = lod_gr;
                new_i += 1;
            }
        }

        lod_grs2.truncate(new_i);

        self.lod_grs = lod_grs2;
    }
}

/// Represents the result of `line_gr_subdiv_incl`.
#[derive(Debug, Clone)]
struct SubdivResult {
    /// The subdivided index range.
    subdived_range: Range<Index>,
    /// The starting line coordinate of the subdivided range. This point does
    /// not move during subdivision.
    pos: Size,
    /// The total size of the subdivided range before subdivision. The line
    /// coordinate range can be calculated as `pos..pos + old_size`.
    old_size: Size,
    /// The total size of the subdivided range after subdivision. The line
    /// coordinate range can be calculated as `pos..pos + new_size`.
    new_size: Size,
}

/// Subdivide `range` from a line group list in order to lower the LOD level.
///
/// `range` is “rounded” to the nearest line group boundaries so that
/// it covers `range`. Returns the rounded range as
/// `SubdivResult::subdived_range`.
fn line_gr_subdiv_incl(
    line_grs: &mut Rope<LineGr, LineOff>,
    new_lod: u8,
    range: Range<Index>,
    model: &dyn LinesetModel,
) -> SubdivResult {
    use rope::{by_key, One::FirstAfter};

    debug_assert!(range.start < line_grs.offset_len().index);
    debug_assert!(range.start < range.end, "{:?}", range);

    let approx = lod_approx(new_lod);
    let new_lod_min_size = *lod_size_range(new_lod).start();

    // The size of the subdivided range before/after subdivision
    let mut old_size = 0;
    let mut new_size = 0;

    // Process `line_gr`. If `line_gr.num_lines >= 2`, it's split into two
    // `LineGr`s. `line_gr` is replaced with the second half, while returning
    // the first half. Otherwise, returns `None`, indicating subdivision did
    // not occur. Even in this case, `line_gr.size` is recalculated if
    // `new_lod == 0`.
    let mut try_split = |line_gr: &mut LineGr, line_off: LineOff| {
        let num_lines1 = (line_gr.num_lines + 1) >> 1;
        let num_lines2 = line_gr.num_lines - num_lines1;

        let new_line_gr;

        if num_lines2 < new_lod_min_size {
            // Can't split - this happens when `new_lod == 0` or
            // `line_gr` is the last line group of a LOD group
            new_line_gr = None;

            // Recalculate the size if we are turning them into LOD 0
            old_size += line_gr.size;
            if !approx {
                debug_assert_eq!(line_gr.num_lines, 1);
                line_gr.size = model.line_total_size(line_off.index..line_off.index + 1, approx);
            }
            new_size += line_gr.size;
        } else {
            let indices = [
                line_off.index,
                line_off.index + num_lines1,
                line_off.index + line_gr.num_lines,
            ];
            let mut sizes = [
                model.line_total_size(indices[0]..indices[1], approx),
                model.line_total_size(indices[1]..indices[2], approx),
            ];

            old_size += line_gr.size;

            // Do not change the total size unless we are turning them into
            // LOD 0
            if approx {
                sizes = divide_size(line_gr.size, sizes);
            }

            new_size += sizes[0] + sizes[1];

            new_line_gr = Some(LineGr {
                num_lines: num_lines1,
                size: sizes[0],
            });

            *line_gr = LineGr {
                num_lines: num_lines2,
                size: sizes[1],
            };
        }

        new_line_gr
    };

    let mut update_fn = |line_gr: &mut LineGr, line_off: LineOff| {
        let next_index = line_off.index + line_gr.num_lines;
        let new_line_gr = try_split(line_gr, line_off);
        (line_off.index, line_off.pos, new_line_gr, next_index)
    };

    // Process the first line group that overlaps with `range`
    let (start, start_pos, new_line_gr, mut next_index) = line_grs
        .update_with(
            FirstAfter(by_key(LineOff::index, range.start)),
            &mut update_fn,
        )
        .unwrap();

    if let Some(new_line_gr) = new_line_gr {
        line_grs
            .insert_before(new_line_gr, FirstAfter(by_key(LineOff::index, start)))
            .unwrap();
    }

    // Process other line groups that follow
    while next_index < range.end {
        let (_, _, new_line_gr, i) = line_grs
            .update_with(
                FirstAfter(by_key(LineOff::index, next_index)),
                &mut update_fn,
            )
            .unwrap();

        if let Some(new_line_gr) = new_line_gr {
            line_grs
                .insert_before(new_line_gr, FirstAfter(by_key(LineOff::index, next_index)))
                .unwrap();
        }

        next_index = i;
    }

    if lod_approx(new_lod) {
        debug_assert_eq!(old_size, new_size);
    }

    SubdivResult {
        subdived_range: start..next_index,
        pos: start_pos,
        old_size,
        new_size,
    }
}

/// Decimate `range` from a line group list in order to raise the LOD level.
///
/// `range` is “rounded” to the nearest line group boundaries so that
/// it is covered by `range`. Returns the rounded range (if any).
fn line_gr_decim_excl(
    line_grs: &mut Rope<LineGr, LineOff>,
    range: Range<Index>,
) -> Option<Range<Index>> {
    use rope::{by_key, One::FirstAfter};

    debug_assert!(range.start < line_grs.offset_len().index);
    debug_assert!(range.start < range.end, "{:?}", range);

    fn clone_first<T1: Clone, T2>((x1, x2): (&T1, T2)) -> (T1, T2) {
        (x1.clone(), x2)
    }

    // Choose the first line group
    let mut cur = clone_first(
        line_grs
            .get_with_offset(FirstAfter(by_key(LineOff::index, range.start)))
            .unwrap(),
    );

    if cur.1.index < range.start {
        // This line group started before `range.start` - ignore this one and
        // choose the next line group
        let cur_end = cur.1.index + cur.0.num_lines;
        cur = clone_first(line_grs.get_with_offset(FirstAfter(by_key(LineOff::index, cur_end)))?);
    }

    if cur.1.index + cur.0.num_lines > range.end {
        return None;
    }

    let start = cur.1.index;

    loop {
        let cur_end = cur.1.index + cur.0.num_lines;
        if cur_end > range.end {
            // `cur` is outside `range`
            debug_assert_ne!(cur.1.index, start);
            return Some(start..cur.1.index);
        } else if cur_end >= range.end {
            // There's no way `cur2` could fit in `range`
            debug_assert_ne!(cur_end, start);
            return Some(start..cur_end);
        }

        // Pair up two line groups (`cur` and the next one - `cur2`).
        let result = line_grs
            .update_with(
                FirstAfter(by_key(LineOff::index, cur_end)),
                |cur2, cur2_off| {
                    debug_assert_eq!(cur2_off.index, cur_end);

                    let cur2_end = cur2_off.index + cur2.num_lines;
                    if cur2_end > range.end {
                        return None;
                    }

                    // Both are completely inside `range`, so merge them
                    cur2.num_lines += cur.0.num_lines;
                    cur2.size += cur.0.size;

                    Some(cur2_end)
                },
            )
            .unwrap();

        if let Some(i) = result {
            // Delete `cur`
            line_grs
                .remove(FirstAfter(by_key(LineOff::index, cur.1.index)))
                .unwrap();

            // Move on
            debug_assert!(i <= range.end);
            if i >= range.end {
                debug_assert_ne!(i, start);
                return Some(start..i);
            }

            cur = clone_first(
                line_grs
                    .get_with_offset(FirstAfter(by_key(LineOff::index, i)))
                    .unwrap(),
            );
        } else {
            // Couldn't merge because `cur2` was outside `range`.
            debug_assert_ne!(cur_end, start);
            return Some(start..cur_end);
        }
    }
}

/// Get how many lines outside a viewport are included in the LOD level `lod`
/// and below.
fn lod_coverage(lod: u8, scale: Index) -> Index {
    debug_assert!(scale >= 0);
    if lod > 0 {
        scale << (lod - 1)
    } else {
        0
    }
}

/// Get the smallest `lod` such that `lod_coverage(lod, scale) >= i`.
fn inverse_lod_coverage(i: Index, scale: Index) -> u8 {
    debug_assert!(scale >= 0);
    debug_assert!(i >= 0);
    if i == 0 {
        0
    } else {
        let i2: Index = (i + scale - 1) / scale - 1;
        ((0 as Index).leading_zeros() - i2.leading_zeros()) as u8 + 1
    }
}

/// Create a desired partition of a lineset containing `len` lines based on the
/// viewports specified by `vps`.
///
/// `cap` is used as the initial capacity of the returned `Vec`.
///
/// Each viewport produces a list of LOD groups like the following:
///
/// ```text
/// LOD groups 1:                                   viewport
///                                                   <-->
///  ,------------+-------------------------+--------+----+--------+-------------,
///  | 3          | 2                       | 1      | 0  | 1      | 2           |
///  '------------+-------------------------+--------+----+--------+-------------'
/// ```
///
/// This function calculates it for each supplied viewport, and combines the
/// lists by calculating the minimum LOD for each continuous range.
///
/// ```text
/// LOD groups 2: viewport
///                 <-->
///  ,----+--------+----+--------+-------------+---------------------------------,
///  | 2  | 1      | 0  | 1      | 2           | 3                               |
///  '----+--------+----+--------+-------------+---------------------------------'
///
/// LOD groups (combined):
///                 <-->                              <-->
///  ,----+--------+----+--------+----------+--------+----+--------+-------------,
///  | 2  | 1      | 0  | 1      | 2        | 1      | 0  | 1      | 2           |
///  '----+--------+----+--------+----------+--------+----+--------+-------------'
/// ```
///
/// `vps.len()` is restricted to a certain range (see assertions inside).
fn lod_grs_from_vps(
    len: Index,
    cap: usize,
    vps: impl Iterator<Item = Range<Index>> + ExactSizeIterator,
) -> Vec<LodGr> {
    if len == 0 {
        return Vec::new();
    }

    /// The properties of a viewport.
    struct Vp {
        scale: Index,
        range: [Index; 2],
        _pad: Index,
    }

    /// An upcoming endpoint.
    struct Ep {
        /// The location of the endpoint. This value is calculated as:
        ///  - `vp.range[0] - lod_coverage(lod, vp.scale)` if `past == false`
        ///  - `vp.range[1] + lod_coverage(lod, vp.scale)` if `past == true`
        index: Index,
        vp_i: u8,
        /// The LOD level after the endpoint.
        lod: u8,
        past: bool,
    }

    impl PartialEq for Ep {
        fn eq(&self, other: &Self) -> bool {
            self.index == other.index
        }
    }
    impl Eq for Ep {}
    impl PartialOrd for Ep {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            self.index.partial_cmp(&other.index).map(Ordering::reverse)
        }
    }
    impl Ord for Ep {
        fn cmp(&self, other: &Self) -> Ordering {
            // `BinaryHeap` is max-heap, but we want `Ep` with the minimum
            // `index`, so reverse the ordering
            self.index.cmp(&other.index).reverse()
        }
    }

    // Limitation of `Minimultiset` and `vp_i`.
    assert!(vps.len() <= 255, "too many viewports: {}", vps.len());

    // There must be at least one viewport
    assert!(vps.len() > 0, "too few viewports: {}", vps.len());

    // Upcoming boundaries where the LOD level required by a viewport changes.
    let mut eps = BinaryHeap::with_capacity(vps.len());

    // The multiset of LOD levels required by their respective viewports.
    let mut lods = multiset::Minimultiset::new();

    let vps: Vec<_> = vps
        .enumerate()
        .map(|(vp_i, range)| {
            debug_assert!(range.start >= 0);
            debug_assert!(range.end <= len);

            // Decide `scale` used for the viewport. The choice is kinda arbitrary.
            let scale = max(4, (range.end - range.start) / 2);

            // Get the required LOD level at index `0`.
            // (This locates the endpoint at `i` where `i <= 0`.)
            let lod = inverse_lod_coverage(range.start, scale);
            lods.insert(lod);

            // Find the next endpoint.
            let vp_i = vp_i as u8;
            let ep = if lod == 0 {
                Ep {
                    index: range.end,
                    vp_i,
                    lod: 1,
                    past: true,
                }
            } else {
                Ep {
                    index: range.start - lod_coverage(lod - 1, scale),
                    vp_i,
                    lod: lod - 1,
                    past: false,
                }
            };
            debug_assert!(ep.index >= 0);
            eps.push(ep);

            Vp {
                scale,
                range: [range.start, range.end],
                _pad: 0,
            }
        })
        .collect();
    let vps = &vps[..];

    let mut lod_grs = Vec::with_capacity(cap);
    let mut last_index = 0;
    let mut last_lod = BAD_LOD; // impossible LOD value

    loop {
        let ep = eps.pop().unwrap();

        // There might be more than one `Ep`s at a single location (`last_index`)
        // and the LOD level must incorporate all of such `Ep`s before
        // finalizing a `LodGr`. So check if we are moving forward. If we are,
        // finalize and add `LodGr` for `last_index`.
        if ep.index > last_index {
            let lod = lods.min();

            // Do not emit redundant endpoints
            if lod != last_lod {
                lod_grs.push(LodGr {
                    index: last_index,
                    lod,
                });
                last_lod = lod;
            }

            last_index = ep.index;
        }

        // Reached the end of the lineset?
        if ep.index >= len {
            break;
        }

        let vp = &vps[ep.vp_i as usize];

        // Update `lods`
        let past_flag_to_delta = |f: bool| (f as i8) * 2 - 1;
        let lod_delta = past_flag_to_delta(ep.past);
        let old_lod = ep.lod.wrapping_sub(lod_delta as u8);

        lods.remove(old_lod);
        lods.insert(ep.lod);

        // Find the next endpoint
        let next_past = if ep.lod == 0 {
            debug_assert_eq!(ep.past, false);
            true
        } else {
            ep.past
        };

        let next_lod_delta = past_flag_to_delta(next_past);
        let next_lod = ep.lod.wrapping_add(next_lod_delta as u8);

        // if next_past: index = vp.range[1] + lod_coverage(lod - 1)
        //           range      lod_coverage
        //     .................. ........
        //     [ 0              ] [ 1    ] [ 2     ]
        //                                ^
        //                        past = true, lod = 2
        //
        // otherwise: index = vp.range[0] - lod_coverage(lod)
        //             lod_coverage     range
        //               ........ ..................
        //     [ 2     ] [ 1    ] [ 0              ]
        //              ^
        //      past = false, lod = 1
        let coverage = lod_coverage(next_lod - next_past as u8, vp.scale);
        let next_index = vp.range[next_past as usize] + coverage * next_lod_delta as Index;

        let next_ep = Ep {
            index: next_index,
            vp_i: ep.vp_i,
            lod: next_lod,
            past: next_past,
        };

        eps.push(next_ep);
    }

    debug_assert_eq!(lod_grs.is_empty(), false);

    lod_grs
}

/// Create an iterator over a list of `LodGr`s. In addition to `LodGr`s, it
/// also returns their respective ending points (`LodGr` itself only stores
/// the starting point).
fn iter_lod_gr_with_end(len: Index, lod_grs: &[LodGr]) -> IterLodGrWithEnd<'_> {
    IterLodGrWithEnd(lod_grs.iter().eager_peekable(), len)
}

#[derive(Clone)]
struct IterLodGrWithEnd<'a>(EagerPeekable<std::slice::Iter<'a, LodGr>>, Index);

impl<'a> Iterator for IterLodGrWithEnd<'a> {
    type Item = (LodGr, Index);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|&gr1| {
            if let Some(gr2) = self.0.peek() {
                (gr1, gr2.index)
            } else {
                (gr1, self.1)
            }
        })
    }
}

/// Get the ending point of the LOD group `lod_grs[i]`.
fn lod_gr_end(len: Index, lod_grs: &[LodGr], i: usize) -> Index {
    if let Some(gr) = lod_grs.get(i + 1) {
        gr.index
    } else {
        len
    }
}

impl Lineset {
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

            let mut iter = iter.eager_peekable();
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

    /// Find a set of consecutive line groups included in the specified
    /// viewport.
    ///
    /// Specifically, it returns an iterator producing `(line_size, num_lines)`,
    /// the index range, and the line coordinate range.
    ///
    /// If the lineset is well-grouped for `vp`, the produced `num_lines` is
    /// always `1`, thus every returned line group represents a single line.
    pub fn range(
        &self,
        vp: Range<Size>,
    ) -> (
        impl Iterator<Item = (Size, Index)> + DoubleEndedIterator + '_,
        Range<Index>,
        Range<Size>,
    ) {
        use rope::{
            range_by_key,
            Edge::{Ceil, Floor},
        };

        let (iter, range) = self
            .line_grs
            .range(range_by_key(LineOff::pos, Floor(vp.start)..Ceil(vp.end)));

        (
            iter.map(|line_gr| (line_gr.size, line_gr.num_lines)),
            range.start.index..range.end.index,
            range.start.pos..range.end.pos,
        )
    }

    /// Check if the lineset is well-grouped for `vp_by_pos`. In addition,
    /// returns the line index range `vp` encompasses.
    pub fn is_well_grouped(&self, vp_by_pos: Range<Size>) -> (bool, Range<Index>) {
        use rope::{
            range_by_key,
            Edge::{Ceil, Floor},
        };

        // Convert `Range<Size>` to `Range<Index>`. This might be
        // overconservative if they cross large line groups, but that's okay.
        let vp_by_idx = {
            let (_, range) = self.line_grs.range(range_by_key(
                LineOff::pos,
                Floor(vp_by_pos.start)..Ceil(vp_by_pos.end),
            ));

            range.start.index..range.end.index
        };

        if vp_by_idx.start >= vp_by_idx.end {
            // Always well-grouped for an empty set.
            return (true, vp_by_idx);
        }

        let lod_gr1_i = match self
            .lod_grs
            .binary_search_by_key(&vp_by_idx.start, |g| g.index)
        {
            Ok(i) => i,
            Err(i) => {
                if vp_by_idx.start == self.num_lines() {
                    i
                } else {
                    i - 1
                }
            }
        };
        let lod_gr2_i = match self
            .lod_grs
            .binary_search_by_key(&vp_by_idx.end, |g| g.index)
        {
            Ok(i) => i,
            Err(i) => i,
        };

        let all_lod0 = self.lod_grs[lod_gr1_i..lod_gr2_i]
            .iter()
            .all(|gr| gr.lod == 0);

        (all_lod0, vp_by_idx)
    }
}

fn vec_remove_range(v: &mut Vec<impl Clone>, range: Range<usize>) {
    if range.len() == 0 {
        return;
    }

    for i in range.start..v.len() - range.len() {
        v[i] = v[i + range.len()].clone();
    }
    v.truncate(v.len() - range.len());
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

    #[test]
    fn test_lod_coverage() {
        const SCALE: Index = 3;
        for i in 0..1000 {
            dbg!(i);
            let lod = dbg!(inverse_lod_coverage(i, SCALE));
            assert!(dbg!(lod_coverage(lod, SCALE)) >= i);
            if lod > 0 {
                assert!(dbg!(lod_coverage(lod - 1, SCALE)) < i);
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
        fn line_total_size(&self, range: Range<Index>, approx: bool) -> Size {
            self.pos(range.end) - self.pos(range.start) + (approx as Size * 9)
        }
    }

    #[test]
    fn insert_to_empty() {
        for i in 0..16 {
            let mut lineset = Lineset::new();
            lineset.validate();

            let range = dbg!(lineset.insert(&TestModel, 0..i));
            dbg!(&lineset);
            lineset.validate();

            if i == 0 {
                assert_eq!(range, None);
            } else {
                assert!(
                    range == Some(0..TestModel.line_total_size(0..i, false))
                        || range == Some(0..TestModel.line_total_size(0..i, true))
                );
            }
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

        fn next_range_u64(&mut self, range: Range<u64>) -> u64 {
            let x = self.next() as u64 | ((self.next() as u64) << 32);
            (x - 1) % (range.end - range.start) + range.start
        }

        /// Create a `Lineset` for testing.
        fn next_lineset(&mut self, lod: u8) -> Lineset {
            let mut lineset = Lineset::new();

            // Prepare the initial state
            let size_range = lod_size_range(lod);
            for _ in 0..4 {
                lineset.lod_grs.push(LodGr {
                    index: lineset.line_grs.offset_len().index,
                    lod,
                });

                let num_line_grs = self.next_range(0..3);
                for _ in 0..num_line_grs {
                    let line_gr_len = self
                        .next_range(*size_range.start() as u32..*size_range.end() as u32 + 1)
                        as _;
                    lineset.line_grs.push_back(LineGr {
                        num_lines: line_gr_len,
                        size: 1,
                    });
                }

                let line_gr_len = self.next_range(1..*size_range.end() as u32 + 1) as _;
                lineset.line_grs.push_back(LineGr {
                    num_lines: line_gr_len,
                    size: 1,
                });
            }

            dbg!(&lineset);
            lineset.validate();

            lineset
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

            let lineset = rng.next_lineset(lod);

            // Try insertion
            for pos in 0..=lineset.line_grs.offset_len().index {
                for &count in &[1, 2, 3, 4, 10] {
                    dbg!(pos..pos + count);
                    let mut lineset = lineset.clone();
                    let len = lineset.line_grs.offset_len().index;
                    let size = lineset.line_grs.offset_len().pos;

                    let range = dbg!(lineset.insert(&TestModel, pos..pos + count));
                    dbg!(&lineset);

                    lineset.validate();
                    assert_eq!(lineset.line_grs.offset_len().index, len + count);

                    let range = range.unwrap();
                    let inserted_size = range.end - range.start;
                    assert_eq!(lineset.line_grs.offset_len().pos, size + inserted_size);
                }
            }
        }
    }

    #[test]
    fn remove() {
        let mut rng = Xorshift32(0xdeadbeef);

        for _ in 0..100 {
            rng.next();
        }

        for lod in [0, 2].iter().flat_map(|&i| std::iter::repeat(i).take(4)) {
            dbg!(lod);

            let lineset = rng.next_lineset(lod);

            // Try removal
            for pos1 in 0..=lineset.line_grs.offset_len().index {
                for pos2 in pos1..=lineset.line_grs.offset_len().index {
                    dbg!(pos1..pos2);
                    let mut lineset = lineset.clone();
                    let len = lineset.line_grs.offset_len().index;
                    let size = lineset.line_grs.offset_len().pos;

                    let range = dbg!(lineset.remove(&TestModel, pos1..pos2));
                    dbg!(&lineset);

                    lineset.validate();
                    assert_eq!(lineset.line_grs.offset_len().index, len - (pos2 - pos1));

                    let removed_size = if let Some(range) = range {
                        range.end - range.start
                    } else {
                        0
                    };
                    assert_eq!(lineset.line_grs.offset_len().pos, size - removed_size);
                }
            }
        }
    }

    #[test]
    fn test_lod_grs_from_vps_empty() {
        let out = lod_grs_from_vps(0, 8, [].iter().cloned());
        assert_eq!(out, Vec::new());

        let out = lod_grs_from_vps(0, 8, [0..0].iter().cloned());
        assert_eq!(out, Vec::new());

        let out = lod_grs_from_vps(0, 8, [0..0, 0..0].iter().cloned());
        assert_eq!(out, Vec::new());
    }

    #[test]
    fn test_lod_grs_from_vps_small() {
        for len in 1..10 {
            for i1 in 0..=len {
                for i2 in i1..=len {
                    test_lod_grs_from_vps_one(len, &[i1..i2]);
                    for i3 in 0..=len {
                        for i4 in i3..=len {
                            test_lod_grs_from_vps_one(len, &[i1..i2, i3..i4]);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_lod_grs_from_vps_longer() {
        const SCALE: Index = 1 << 59;
        for len in 1..8 {
            for i1 in 0..=len {
                for i2 in i1..=len {
                    test_lod_grs_from_vps_one(len * SCALE, &[i1 * SCALE..i2 * SCALE]);
                    for i3 in 0..=len {
                        for i4 in i3..=len {
                            test_lod_grs_from_vps_one(
                                len * SCALE,
                                &[i1 * SCALE..i2 * SCALE, i3 * SCALE..i4 * SCALE],
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_lod_grs_from_vps_many_vps() {
        let mut rng = Xorshift32(1000000);
        for &len in &[10000000000000] {
            for _ in 0..100 {
                let vps: Vec<_> = (0..255)
                    .map(|_| {
                        let start = rng.next_range_u64(0..len as u64 + 1);
                        let end = rng.next_range_u64(start..len as u64 + 1);
                        start as Index..end as Index
                    })
                    .collect();

                test_lod_grs_from_vps_one(len, &vps);
            }
        }
    }

    fn test_lod_grs_from_vps_one(len: Index, viewports: &[Range<Index>]) {
        dbg!((len, viewports));
        let out = lod_grs_from_vps(len, 8, viewports.iter().cloned());
        dbg!(&out);

        assert_eq!(out.is_empty(), false);
        assert_eq!(out[0].index, 0);

        for win in out.windows(2) {
            assert!(win[0].index < win[1].index);
        }

        for vp in viewports.iter() {
            if vp.start == vp.end {
                continue;
            }

            let i = max(vp.start, 0);
            let gr_i = match out.binary_search_by_key(&i, |gr| gr.index) {
                Ok(i) => i,
                Err(i) => i - 1,
            };

            // LOD groups in `vp` must have LOD level 0
            assert_eq!(out[gr_i].lod, 0);

            let gr_end = if let Some(gr) = out.get(gr_i + 1) {
                gr.index
            } else {
                len
            };

            assert!(gr_end >= vp.end);
        }

        // Regions outside the viewports must have LOD level > 0.
        let min: Index = viewports.iter().map(|vp| vp.start).min().unwrap();
        let max: Index = viewports.iter().map(|vp| vp.end).max().unwrap();
        if min > 0 {
            let gr_i = match out.binary_search_by_key(&min, |gr| gr.index) {
                Ok(i) => i - 1,
                Err(i) => i - 1,
            };
            assert_ne!(out[gr_i].lod, 0);
        }
        if max < len {
            let gr_i = match out.binary_search_by_key(&max, |gr| gr.index) {
                Ok(i) => i,
                Err(i) => i - 1,
            };
            assert_ne!(out[gr_i].lod, 0);
        }
    }

    fn displace_vp(i: &mut Size, old_pos_range: &Range<Size>, new_pos_range: &Range<Size>) {
        if old_pos_range.end <= *i {
            *i += new_pos_range.end - old_pos_range.end;
        } else if new_pos_range.end <= *i {
            *i = new_pos_range.end;
        }
    }

    #[test]
    fn test_regroup_empty() {
        let mut lineset = Lineset::new();
        lineset.regroup(&TestModel, &[0..0], &mut ());
    }

    #[test]
    fn test_regroup1() {
        const NUM_LINES: Index = 100;

        let mut lineset = Lineset::new();

        lineset.insert(&TestModel, 0..NUM_LINES);
        dbg!(&lineset);

        let len = lineset.line_grs.offset_len().pos;

        for i in 0..=100 {
            let pos = len * i / 100;
            let mut lineset = lineset.clone();

            let size = lineset.line_grs.offset_len().pos;
            let mut expected_size = size;

            let vp = pos..pos + 1;
            println!("Regrouping using viewport = {:?}", vp);

            let mut vp_displaced = vp.clone();

            lineset.regroup(&TestModel, &[vp.clone()], &mut |range: Range<Index>,
                                                             old_pos_range: Range<
                Size,
            >,
                                                             new_pos_range: Range<
                Size,
            >| {
                dbg!((&range, &old_pos_range, &new_pos_range));
                expected_size += new_pos_range.end - new_pos_range.start;
                expected_size -= old_pos_range.end - old_pos_range.start;
                displace_vp(&mut vp_displaced.start, &old_pos_range, &new_pos_range);
                displace_vp(&mut vp_displaced.end, &old_pos_range, &new_pos_range);
            });
            dbg!(&lineset);

            lineset.validate();
            assert!(lineset.lod_grs.len() > 3); // we expect to see a few LOD groups
            assert_eq!(lineset.line_grs.offset_len().index, NUM_LINES);

            let (lines_in_view, index_range, pos_range) = lineset.range(vp_displaced.clone());
            dbg!((&vp_displaced, index_range, pos_range));
            assert!(
                { lines_in_view }.all(|(_size, num_lines)| num_lines == 1),
                "found a non-precise line group in view"
            );

            assert!(lineset.is_well_grouped(vp_displaced.clone()).0);
        }
    }

    #[test]
    fn test_regroup2() {
        const NUM_LINES: Index = 100;

        let mut lineset = Lineset::new();
        lineset.insert(&TestModel, 0..NUM_LINES);
        dbg!(&lineset);

        let mut rng = Xorshift32(100000);

        for i in 0..=1000 {
            let len = lineset.line_grs.offset_len().pos;
            let num_vps = if i == 1000 {
                // Make sure the viewports do not cover entire the lineset,
                // so that the last assertion makes sense
                1
            } else {
                rng.next_range(1..4)
            };
            let vps: Vec<_> = (0..num_vps)
                .map(|_| {
                    let start = rng.next_range_u64(0..len as u64 + 1);
                    let end = rng.next_range_u64(start..len as u64 + 1);
                    let end = start + (end - start) / 4;
                    start as Index..end as Index
                })
                .collect();

            println!("Regrouping using viewports = {:?}", vps);

            let mut vps_displaced = vps.clone();
            let mut expected_size = len;

            lineset.regroup(
                &TestModel,
                &vps,
                &mut |range: Range<Index>,
                      old_pos_range: Range<Size>,
                      new_pos_range: Range<Size>| {
                    dbg!((&range, &old_pos_range, &new_pos_range));
                    expected_size += new_pos_range.end - new_pos_range.start;
                    expected_size -= old_pos_range.end - old_pos_range.start;

                    for vp in vps_displaced.iter_mut() {
                        displace_vp(&mut vp.start, &old_pos_range, &new_pos_range);
                        displace_vp(&mut vp.end, &old_pos_range, &new_pos_range);
                    }
                },
            );
            dbg!(&lineset);
            lineset.validate();
            // TODO: check other properties, e.g.:
            //       - Does not resize lines in the intersection of old and new
            //         visible portions

            assert_eq!(lineset.line_grs.offset_len().index, NUM_LINES);
            assert_eq!(lineset.line_grs.offset_len().pos, expected_size);

            for vp in vps_displaced.iter() {
                let (lines_in_view, index_range, pos_range) = lineset.range(vp.clone());
                dbg!((vp, index_range, pos_range));
                assert!(
                    { lines_in_view }.all(|(_size, num_lines)| num_lines == 1),
                    "found a non-precise line group in view"
                );

                assert!(lineset.is_well_grouped(vp.clone()).0);
            }
        }

        assert!(lineset.lod_grs.len() > 0);
    }

    #[test]
    fn test_recalculate_size_empty() {
        let mut lineset = Lineset::new();

        for i1 in -5..=5 {
            for i2 in -5..=5 {
                for &skip_approx in [false, true].iter() {
                    lineset.recalculate_size(
                        &TestModel,
                        i1..i2,
                        skip_approx,
                        &mut |_: Range<Index>, _: Range<Size>, _: Range<Size>| {
                            unreachable!();
                        },
                    );
                    lineset.validate();
                }
            }
        }
    }

    struct TestModel2;

    impl LinesetModel for TestModel2 {
        fn line_total_size(&self, range: Range<Index>, approx: bool) -> Size {
            (TestModel.line_total_size(range, approx) + 1) * 100000
        }
    }

    #[test]
    fn test_recalculate_size1() {
        const LEN: Index = 20;

        let mut lineset = Lineset::new();
        lineset.insert(&TestModel, 0..LEN);
        lineset.regroup(&TestModel, &[0..5], &mut ());

        dbg!(&lineset);
        lineset.validate();

        for i1 in 0..=LEN {
            for i2 in i1 + 1..=LEN {
                dbg!(i1..i2);
                let mut lineset = lineset.clone();
                let size = lineset.line_grs.offset_len().pos;
                let mut expected_size = size;

                lineset.recalculate_size(
                    &TestModel2,
                    i1..i2,
                    false,
                    &mut |range: Range<Index>,
                          old_pos_range: Range<Size>,
                          new_pos_range: Range<Size>| {
                        dbg!((&range, &old_pos_range, &new_pos_range));
                        expected_size += new_pos_range.end - new_pos_range.start;
                        expected_size -= old_pos_range.end - old_pos_range.start;
                    },
                );

                dbg!(&lineset);
                lineset.validate();

                assert_eq!(lineset.line_grs.offset_len().index, LEN);
                assert_eq!(lineset.line_grs.offset_len().pos, expected_size);

                // `TestModel2` returns much larger values, so the affected
                // range is clearly distinguishable
                let mut i = 0;
                for line_gr in lineset.line_grs.iter() {
                    let affected = i < i2 && i + line_gr.num_lines > i1;

                    assert_eq!(affected, line_gr.size > 10000);

                    i += line_gr.num_lines;
                }
            }
        }
    }
}
