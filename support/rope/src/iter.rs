//! Iterators for `Rope`
use arrayvec::ArrayVec;
use std::ops::Range;

use super::{Cursor, EdgeType, NodeRef, Offset, Rope, RopeRangeBounds, ToOffset, CURSOR_LEN};

impl<T, O> Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    fn range_with_cursor(
        &self,
        mut range: impl RopeRangeBounds<O>,
    ) -> RopeRange<IterWithCursor<'_, T, O>, O> {
        let mut iter = IterWithCursor {
            rope: self,
            path1: ArrayVec::new(),
            indices1: [EMPTY_INDEX; CURSOR_LEN],
            path2: ArrayVec::new(),
            indices2: [EMPTY_INDEX; CURSOR_LEN],
            start_unbounded: true,
        };

        if self.is_empty() {
            return (iter, O::zero()..O::zero());
        }

        let (start, mut end);

        match range.start_ty() {
            None => {
                start = O::zero();
            }
            Some(ty) => {
                let search_result = match ty {
                    EdgeType::Floor => {
                        self.inclusive_lower_bound_by(|probe| range.start_cmp(probe))
                    }
                    EdgeType::Ceil => self.inclusive_upper_bound_by(|probe| range.start_cmp(probe)),
                };
                if let Some((c, mut o)) = search_result {
                    // TODO: This is utterly inefficient. Maybe `Cursor` should use
                    //       the "empty path" one-past-end representation
                    iter.start_unbounded = false;
                    if c == self.end() {
                        // Now `path1` is one-past-end
                    } else {
                        self.cursor_to_iter_cursor(c, &mut iter.path1, &mut iter.indices1);

                        if ty == EdgeType::Ceil {
                            // Move the cursor by one element
                            let elem = iter_cursor_get(&iter.path1, &iter.indices1);
                            o += elem.to_offset();

                            iter_cursor_move_forward(&mut iter.path1, &mut iter.indices1);
                        }
                    }
                    start = o;
                } else {
                    start = O::zero();
                }
            }
        };

        match range.end_ty() {
            None => {
                end = self.len.clone();
            }
            Some(ty) => {
                let search_result = match ty {
                    EdgeType::Floor => self.inclusive_lower_bound_by(|probe| range.end_cmp(probe)),
                    EdgeType::Ceil => self.inclusive_upper_bound_by(|probe| range.end_cmp(probe)),
                };
                if let Some((c, mut o)) = search_result {
                    // TODO: This is utterly inefficient. `Cursor` should use
                    //       the "empty path" one-past-end representation
                    if c != self.end() {
                        self.cursor_to_iter_cursor(c, &mut iter.path2, &mut iter.indices2);

                        if ty == EdgeType::Ceil {
                            // Move the cursor by one element
                            let elem = iter_cursor_get(&iter.path2, &iter.indices2);
                            o += elem.to_offset();

                            iter_cursor_move_forward(&mut iter.path2, &mut iter.indices2);
                        }
                    }
                    end = o;
                } else {
                    self.cursor_to_iter_cursor(self.begin(), &mut iter.path2, &mut iter.indices2);
                    end = O::zero();
                }
            }
        };

        // If `end` < `start`, clamp `end`
        if !iter.start_unbounded && iter.indices2 < iter.indices1 {
            iter.indices2 = iter.indices1.clone();
            iter.path2 = iter.path1.clone();
            end = start.clone();
        }

        (iter, start..end)
    }

    /// Construct a double-ended iterator over the elements in the rope.
    ///
    /// This method is a shorthand for `self.range(..).0`.
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a T> + DoubleEndedIterator + 'a {
        self.range(..).0
    }

    /// Construct a double-ended iterator over a sub-range of elements in the
    /// rope.
    ///
    /// Returns a pair of an iterator and a `Range<O>` representing the range of
    /// offset values of the sub-range.
    ///
    /// This method performs a search operation, whose time complexity is
    /// O(log n), for each unbounded endpoint.
    ///
    /// # Examples
    ///
    /// ```
    /// use rope::{Rope, by_ord, Edge::{Floor, Ceil}};
    /// let rope: Rope<String> = [
    ///     "Pony ", "ipsum ", "dolor ", "sit ", "amet ", "ms ",
    /// ].iter().map(|x|x.to_string()).collect();
    ///
    /// //        Floor(7) ┐         Ceil(7) ┐  Floor,Ceil(17) ┐
    /// //                 │                 │                 │
    /// //  0  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16 17 18 19 20 21
    /// //  ┌──────────────┬─────────────────┬─────────────────┬───────────┐
    /// //  │P ┊o ┊n ┊y ┊  │i ┊p ┊s ┊u ┊m ┊  │d ┊o ┊l ┊o ┊r ┊  │s ┊i ┊t ┊  │
    /// //  └──────────────┴─────────────────┴─────────────────┴───────────┘
    ///
    /// // Using endpoint values:
    /// let (iter, range) = rope.range(by_ord(&(Floor(7)..Floor(17))));
    /// assert_eq!(
    ///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
    ///     &["ipsum ", "dolor "],
    /// );
    /// assert_eq!(range, 5..17);
    ///
    /// let (iter, range) = rope.range(by_ord(&(Ceil(7)..Floor(17))));
    /// assert_eq!(
    ///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
    ///     &["dolor "],
    /// );
    /// assert_eq!(range, 11..17);
    ///
    /// // Using comparators:
    /// let (iter, range) = rope.range((
    ///     Ceil(|probe: &isize| probe.cmp(&7)),
    ///     Floor(|probe: &isize| probe.cmp(&17)),
    /// ));
    /// assert_eq!(
    ///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
    ///     &["dolor "],
    /// );
    /// assert_eq!(range, 11..17);
    /// ```
    pub fn range<'a>(
        &'a self,
        range: impl RopeRangeBounds<O>,
    ) -> RopeRange<impl Iterator<Item = &'a T> + DoubleEndedIterator + 'a, O> {
        let (iter, range) = self.range_with_cursor(range);
        (iter.map(|(_, item)| item), range)
    }

    /// Convert `Cursor` for an internal representation used by `IterWithCursor`.
    fn cursor_to_iter_cursor<'a>(
        &'a self,
        cursor: Cursor,
        path: &mut Path<'a, T, O>,
        indices: &mut [u8; CURSOR_LEN],
    ) {
        *indices = [EMPTY_INDEX; CURSOR_LEN];

        let mut cur = &self.root;
        path.clear();
        path.push(cur);
        indices[0] = cursor.indices[0];
        for i in 1..cursor.indices.len() {
            cur = match cur {
                NodeRef::Internal(inode) => &inode.children[indices[i - 1] as usize],
                _ => unreachable!(),
            };
            path.push(cur);
            indices[i] = cursor.indices[i];
        }
    }
}

/// Bundles up an offset range and an iterator.
pub type RopeRange<I, O> = (I, Range<O>);

type Path<'a, T, O> = ArrayVec<[&'a NodeRef<T, O>; CURSOR_LEN]>;

const EMPTY_INDEX: u8 = 0xff;

struct IterWithCursor<'a, T, O> {
    rope: &'a Rope<T, O>,
    // (The first endpoint)
    //   Points the first element in the range.
    //   `start_unbounded` indicates the endpoint is unbounded - essentially a
    //   lazily-evaluated `begin()` (except for the case `rope.is_empty()`).
    //
    /// The parent `NodeRef` for each index in `indices`.
    /// `[]` represents the one-past-end element. (This is a different
    /// convention than the one used by `end()`)
    path1: Path<'a, T, O>,
    /// The same as `Cursor::indices`, but does not have length information.
    /// Extra elements must be filled with `EMPTY_INDEX` to make comparisons
    /// quick.
    indices1: [u8; CURSOR_LEN],

    // (The second endpoint)
    //   Points the first element following the range. The element can be a
    //   one-past-end element.
    path2: Path<'a, T, O>,
    indices2: [u8; CURSOR_LEN],

    start_unbounded: bool,
    // There isn't `end_unbounded` because `path2` representing a one-past-end
    // element is trivial - a zero-element array.
}

// This will probably be reduced to one or two instructions by inlining
#[inline]
fn iter_cursor_to_cursor<T, O>(path: &Path<'_, T, O>, indices: &[u8; CURSOR_LEN]) -> Cursor {
    let mut cursor = Cursor {
        indices: ArrayVec::from(*indices),
        _pad: Default::default(),
    };
    cursor.indices.truncate(path.len());
    cursor
}

/// Move the cursor forward.
///
/// `path` must not point a one-past-end element.
fn iter_cursor_move_forward<T, O>(path: &mut Path<'_, T, O>, indices: &mut [u8; CURSOR_LEN]) {
    let level = path.len() - 1;
    indices[level] += 1;

    match path[level] {
        NodeRef::Leaf(leaf) => {
            // Don't want `movzx r, r`, so compare between `u8`s
            // Are we still in the current leaf node?
            if indices[level] < leaf.len() as u8 {
                return;
            }
        }
        _ => unreachable!(),
    }

    // The current leaf is exhausted
    indices[level] = EMPTY_INDEX;
    path.pop();

    while path.len() > 0 {
        let level = path.len() - 1;
        indices[level] += 1;

        // Find the next child
        let children = match path[level] {
            NodeRef::Internal(inode) => &inode.children,
            _ => unreachable!(),
        };
        let i = indices[level] as usize;

        if i >= children.len() {
            // No more children to iterate
            indices[level] = EMPTY_INDEX;
            path.pop();
        } else {
            // Found a sibling node. Now, find the first element in it
            indices[level] = i as _;
            let mut cur = &children[i];
            path.push(cur); // `[level + 1]`
            loop {
                let level = path.len() - 1;
                indices[level] = 0;
                match cur {
                    NodeRef::Internal(inode) => {
                        cur = inode.children.first().unwrap();
                        path.push(cur);
                    }
                    NodeRef::Leaf(_) => {
                        break;
                    }
                    NodeRef::Invalid => unreachable!(),
                }
            }
            break;
        }
    }
}

/// Move the cursor backward.
///
/// `path` must not point a one-past-end element.
fn iter_cursor_move_backward<'a, T, O>(path: &mut Path<'a, T, O>, indices: &mut [u8; CURSOR_LEN]) {
    let level = path.len() - 1;

    let (i, overflow) = indices[level].overflowing_sub(1);
    indices[level] = i;
    if !overflow {
        return;
    }

    // The current leaf is exhausted
    indices[level] = EMPTY_INDEX;
    path.pop();

    while path.len() > 0 {
        let level = path.len() - 1;

        let (i, overflow) = indices[level].overflowing_sub(1);
        indices[level] = i;

        // Find the next child
        let children = match path[level] {
            NodeRef::Internal(inode) => &inode.children,
            _ => unreachable!(),
        };

        if overflow {
            // No more children to iterate
            indices[level] = EMPTY_INDEX;
            path.pop();
        } else {
            // Found a sibling node. Now, find the first element in it
            let mut cur = &children[i as usize];
            path.push(cur); // `[level + 1]`
            loop {
                let level = path.len() - 1;
                match cur {
                    NodeRef::Internal(inode) => {
                        indices[level] = (inode.children.len() - 1) as u8;
                        cur = &inode.children.last().unwrap();
                        path.push(cur);
                    }
                    NodeRef::Leaf(elements) => {
                        indices[level] = (elements.len() - 1) as u8;
                        break;
                    }
                    NodeRef::Invalid => unreachable!(),
                }
            }
            break;
        }
    }
}

/// Get the leaf element.
fn iter_cursor_get<'a, T, O>(path: &Path<'a, T, O>, indices: &[u8; CURSOR_LEN]) -> &'a T {
    let level = path.len() - 1;
    match path[level] {
        NodeRef::Leaf(leaf) => &leaf[indices[level] as usize],
        _ => unreachable!(),
    }
}

impl<'a, T, O> Iterator for IterWithCursor<'a, T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    type Item = (Cursor, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        // Force the evaluation of the first endpoint
        if self.start_unbounded {
            if self.rope.is_empty() {
                // The one-past-end cursor returned by `begin()` is not compatible
                // with `IterWithCursor`'s internal representation!
                return None;
            } else {
                self.rope.cursor_to_iter_cursor(
                    self.rope.begin(),
                    &mut self.path1,
                    &mut self.indices1,
                );
            }
            self.start_unbounded = false;
        }

        // The iteration completes when the endpoints meet.
        if self.indices1 == self.indices2 {
            return None;
        }

        // Convert the internal pointer to `Cursor`
        let cursor = iter_cursor_to_cursor(&self.path1, &self.indices1);

        // Get the current element
        let elem = iter_cursor_get(&self.path1, &self.indices1);

        // Advance the cursor
        iter_cursor_move_forward(&mut self.path1, &mut self.indices1);

        Some((cursor, elem))
    }
}

impl<'a, T, O> DoubleEndedIterator for IterWithCursor<'a, T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        // The iteration completes when the endpoints meet.
        if !self.start_unbounded && self.indices1 == self.indices2 {
            return None;
        }

        // Advance the cursor
        if self.path2.len() == 0 {
            if self.rope.is_empty() {
                // The one-past-end cursor returned by `begin()` is not compatible
                // with `IterWithCursor`'s internal representation!
                return None;
            } else {
                self.rope.cursor_to_iter_cursor(
                    self.rope.last_cursor(),
                    &mut self.path2,
                    &mut self.indices2,
                );
            }
        } else {
            iter_cursor_move_backward(&mut self.path2, &mut self.indices2);
        }

        if self.path2.len() == 0 {
            // `path2` pointed the first element, and now points the one-past-
            // end element. That means we have no more elements to return.

            // We mutated `path2` in-place and it currently points the
            // one-past-end element.  When next time `next_back` is called,
            // the iterator starts again from the end of the rope, hence
            // `!FusedIterator`.
            return None;
        }

        // Convert the internal pointer to `Cursor`
        let cursor = iter_cursor_to_cursor(&self.path2, &self.indices2);

        // Get the current element
        let elem = iter_cursor_get(&self.path2, &self.indices2);

        Some((cursor, elem))
    }
}
