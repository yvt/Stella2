//! This crate implements a variant of [the rope data structure] inspired from
//! [the B+ tree].
//!
//! [the rope data structure]: https://en.m.wikipedia.org/wiki/Rope_(data_structure)
//! [the B+ tree]: https://en.wikipedia.org/wiki/B+_tree
//!
//! Logically, it can be modeled as a sequence of elements, each having a value
//! representing the length of type implementing [`Offset`] (calculated by
//! `<T as ToOffset<O>>::to_offset`). The offset value of an element is
//! calculated as a sum of all preceding elements' length.
//!
//! Offset values can have multiple aspects like byte offset and grapheme
//! cluster count. Aspects can be combined using the type system, as exemplified
//! by [`IndexOffset`]`<T>`, which adds an element index to an existing `Offset`.
//! To do a search operation, one of aspects must be chosen.
//!
//! ```text
//! bytes:    0  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16 17 18 19 20 21
//!           ┌──────────────┬─────────────────┬─────────────────┬───────────┐
//!           │P ┊o ┊n ┊y ┊  │i ┊p ┊s ┊u ┊m ┊  │d ┊o ┊l ┊o ┊r ┊  │s ┊i ┊t ┊  │
//!           └──────────────┴─────────────────┴─────────────────┴───────────┘
//! elements: 0              1                 2                 3           4
//! ```
//!
//! It supports the following operations:
//!
//!  - O(log n) insertion at an arbitrary location.
//!  - O(log n) removal of an arbitrary location.
//!  - O(log n) search by an offset comparison function.
//!
//! It does not support indexing like normal arrays. However, it can be added
//! by combining an existing `Offset` with [`IndexOffset`].
use arrayvec::ArrayVec;
use std::cmp::Ordering;

mod iter;
mod misc;
mod offset;
mod ops;
mod sel;
pub use self::{iter::*, offset::*, sel::*};

/// Represents a rope.
///
/// See [the crate documentation](index.html) for more.
#[derive(Clone)]
pub struct Rope<T, O = isize> {
    root: NodeRef<T, O>,
    len: O,
}

/// The minimum number of child nodes of elements in a single node. The actual
/// number varies between `ORDER` and `ORDER * 2`. The root node is exempt from
/// the minimum count limitation.
const ORDER: usize = 1 << ORDER_SHIFT;

const ORDER_SHIFT: u32 = 3;

/// A reference to a node.
#[derive(Debug, Clone)]
enum NodeRef<T, O> {
    Internal(Box<INode<T, O>>),
    /// A leaf node.
    ///
    /// Invariant:
    /// ```text
    /// let min = if node_is_root() { 0 } else { ORDER };
    /// (min..=ORDER * 2).contains(&array_vec.len())
    /// ```
    Leaf(Box<ArrayVec<[T; ORDER * 2]>>),
    Invalid,
}

impl<T, O> NodeRef<T, O> {
    /// Get the number of the node's children.
    fn len(&self) -> usize {
        match self {
            NodeRef::Internal(inode) => inode.children.len(),
            NodeRef::Leaf(elements) => elements.len(),
            NodeRef::Invalid => unreachable!(),
        }
    }

    fn is_internal(&self) -> bool {
        match self {
            NodeRef::Internal(_) => true,
            _ => false,
        }
    }

    fn is_leaf(&self) -> bool {
        match self {
            NodeRef::Leaf(_) => true,
            _ => false,
        }
    }
}

/// A non-leaf node.
#[derive(Debug, Clone)]
struct INode<T, O> {
    /// `offsets[i]` represents the relative offset of `children[i + 1]`
    /// relative to `children[0]`.
    ///
    /// Invariant: `offsets.len() == children.len() - 1 &&`
    /// `offsets[i] == all_elements(children[0..i + 1]).map(to_offset).sum()`
    ///
    /// Why not use `children[i].len()`? Because on a theoretical superscalar
    /// processor with an infinite number of execution pipes, this approach is
    /// faster for most operations. Does it apply to a real processor? Yes, if
    /// `O::add` has a long latency. Also, you can use a binary search.
    offsets: ArrayVec<[O; ORDER * 2 - 1]>,

    /// The child nodes.
    ///
    /// Invariants:
    /// ```text
    /// let min = if node_is_root() { 2 } else { ORDER };
    /// let len_contraint = (min..=ORDER * 2).contains(&children.len());
    ///
    /// let type_constraint = children.iter().all(is_leaf) ||
    ///     children.iter().all(is_internal);
    ///
    /// len_contraint && type_constraint
    /// ```
    children: ArrayVec<[NodeRef<T, O>; ORDER * 2]>,
}

/// The capacity of `Cursor::indices`.
///
/// This defines the maximum depth of the tree because `Cursor` is used address
/// nodes. Supposing `ORDER_SHIFT == 3`, `16` is sufficient to contain circa
/// 2.8×10¹⁴ elements.
/// To cover the entire range of 64-bit `usize`, specify
/// `std::mem::size_of::<usize>() * 8 / ORDER_SHIFT as usize + 1`.
const CURSOR_LEN: usize = 16;

#[derive(Debug, Default, PartialEq)]
struct Cursor {
    /// Each element represents an index into `INode::children` or
    /// `NodeRef::Leaf` at the corresponding level.
    ///
    /// The last element is an index into `NodeRef::Leaf` and can point
    /// the one-past-end element.
    indices: ArrayVec<[u8; CURSOR_LEN]>,

    /// Pad the structure for better code generation at cost of memory
    /// efficiency.
    _pad: [u8; 15 - (CURSOR_LEN + 15) % 16],
}

impl<T, O> Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    /// Construct an empty `Rope`.
    pub fn new() -> Self {
        Self {
            root: NodeRef::Leaf(Box::new(ArrayVec::new())),
            len: O::zero(),
        }
    }

    /// Get the total length (not necessarily the number of elements, unless
    /// `O` is [`Index`]) of the rope.
    pub fn offset_len(&self) -> O {
        self.len.clone()
    }

    /// Return `true` if the rope contains no elements.
    pub fn is_empty(&self) -> bool {
        match &self.root {
            NodeRef::Leaf(leaf) => leaf.is_empty(),
            _ => false,
        }
    }

    /// Insert an element to the back of the rope.
    pub fn push_back(&mut self, x: T) {
        self.insert(x, self.end());
    }

    /// Insert an element to the front of the rope.
    pub fn push_front(&mut self, x: T) {
        self.insert(x, self.begin());
    }

    /// Remove an element from the back of the rope.
    ///
    /// Returns `None` if the rope is empty.
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(self.remove_at(self.last_cursor()))
        }
    }

    /// Remove an element from the front of the rope.
    ///
    /// Returns `None` if the rope is empty.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(self.remove_at(self.begin()))
        }
    }

    /// Get the first element if it exists.
    pub fn first(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(self.get_at(self.begin()))
        }
    }

    /// Get the last element if it exists.
    pub fn last(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(self.get_at(self.last_cursor()))
        }
    }

    /// Get the reference to an element.
    pub fn get(&self, one: One<impl FnMut(&O) -> Ordering>) -> Option<&T> {
        self.find_one(one).map(|(cursor, _)| self.get_at(cursor))
    }

    /// Get the reference to an element and the element's position relative to
    /// the front of the rope.
    ///
    /// # Examples
    ///
    /// ```
    /// use rope::{Rope, by_ord, One::{FirstAfter, LastBefore}};
    /// let rope: Rope<String> = [
    ///     "Pony ", "ipsum ", "dolor ", "sit ", "amet ", "ms ",
    /// ].iter().map(|x|x.to_string()).collect();
    ///
    /// // Using endpoint values:
    /// let (elem, offset) = rope.get_with_offset(FirstAfter(by_ord(7))).unwrap();
    /// assert_eq!((elem.as_str(), offset), ("ipsum ", 5));
    ///
    /// let (elem, offset) = rope.get_with_offset(LastBefore(by_ord(17))).unwrap();
    /// assert_eq!((elem.as_str(), offset), ("dolor ", 11));
    ///
    /// // Using a comparator:
    /// let (elem, offset) = rope
    ///     .get_with_offset(FirstAfter(|probe: &isize| probe.cmp(&7)))
    ///     .unwrap();
    /// assert_eq!((elem.as_str(), offset), ("ipsum ", 5));
    /// ```
    pub fn get_with_offset(&self, one: One<impl FnMut(&O) -> Ordering>) -> Option<(&T, O)> {
        self.find_one(one)
            .map(|(cursor, offset)| (self.get_at(cursor), offset))
    }

    /// Get the mutable reference to an element.
    ///
    /// This is more efficient than `update_with`, but does not allow changing
    /// the element's length (`<T as ToOffset<O>>::to_offset`).
    pub fn get_mut(&mut self, one: One<impl FnMut(&O) -> Ordering>) -> Option<&mut T> {
        self.find_one(one)
            .map(move |(cursor, _)| self.get_mut_at(cursor))
    }

    /// Get the mutable reference to an element and the element's position
    /// relative to the front of the rope.
    ///
    /// This is more efficient than `update_with`, but does not allow changing
    /// the element's length (`<T as ToOffset<O>>::to_offset`).
    pub fn get_mut_with_offset(
        &mut self,
        one: One<impl FnMut(&O) -> Ordering>,
    ) -> Option<(&mut T, O)> {
        self.find_one(one)
            .map(move |(cursor, offset)| (self.get_mut_at(cursor), offset))
    }

    /// Update an element using a given function, possibly changing its length
    /// (`<T as ToOffset<O>>::to_offset`).
    ///
    /// # Examples
    ///
    /// ```
    /// use rope::{Rope, by_ord, One::FirstAfter};
    /// let mut rope: Rope<String> = ["Pony", " ", "ipsum"]
    ///     .iter().map(|x|x.to_string()).collect();
    ///
    /// assert_eq!(rope.get_with_offset(FirstAfter(by_ord(8))).unwrap().1, 5);
    ///
    /// // Replace the first element
    /// rope.update_with(FirstAfter(by_ord(0)), |e, _| *e = "Lorem".to_string())
    ///     .unwrap();
    ///
    /// // The subsequent elements are moved accordingly
    /// assert_eq!(rope.get_with_offset(FirstAfter(by_ord(8))).unwrap().1, 6);
    /// ```
    pub fn update_with<R>(
        &mut self,
        one: One<impl FnMut(&O) -> Ordering>,
        with: impl FnOnce(&mut T, O) -> R,
    ) -> Option<R> {
        self.find_one(one)
            .map(|(cursor, offset)| self.update_at_with(cursor, move |e| with(e, offset)))
    }

    /// Remove an element and return it along with its original element position.
    ///
    /// # Examples
    ///
    /// ```
    /// use rope::{Rope, by_ord, One::FirstAfter};
    /// let mut rope: Rope<String> = ["Pony", " ", "ipsum"]
    ///     .iter().map(|x|x.to_string()).collect();
    ///
    /// assert_eq!(rope.get_with_offset(FirstAfter(by_ord(8))).unwrap().1, 5);
    ///
    /// // Remove the first element
    /// let (elem, offset) = rope.remove(FirstAfter(by_ord(0))).unwrap();
    /// assert_eq!((elem.as_str(), offset), ("Pony", 0));
    ///
    /// // The subsequent elements are moved accordingly
    /// assert_eq!(rope.get_with_offset(FirstAfter(by_ord(4))).unwrap().1, 1);
    /// ```
    pub fn remove(&mut self, one: One<impl FnMut(&O) -> Ordering>) -> Option<(T, O)> {
        self.find_one(one)
            .map(|(cursor, offset)| (self.remove_at(cursor), offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let mut rope: Rope<String> = Rope::new();
        dbg!(&rope.root);
        rope.validate();

        assert!(rope.is_empty());
        assert_eq!(rope.first(), None);
        assert_eq!(rope.last(), None);
        assert_eq!(rope.pop_back(), None);
        assert_eq!(rope.pop_front(), None);
        assert_eq!(rope.offset_len(), 0);
    }

    #[test]
    fn push_back() {
        let mut rope: Rope<String> = Rope::new();
        for i in 0..400 {
            rope.push_back(i.to_string());
            dbg!(&rope.root);
            rope.validate();
        }

        let elems: Vec<u32> = rope.iter().map(|x| x.parse().unwrap()).collect();
        assert_eq!(elems, (0..400).collect::<Vec<u32>>());
    }

    #[test]
    fn push_front() {
        let mut rope: Rope<String> = Rope::new();
        for i in 0..400 {
            rope.push_front(i.to_string());
            dbg!(&rope.root);
            rope.validate();
        }

        let elems: Vec<u32> = rope.iter().map(|x| x.parse().unwrap()).collect();
        assert_eq!(elems, (0..400).rev().collect::<Vec<u32>>());
    }

    #[test]
    fn pop_front() {
        let mut rope: Rope<String> = Rope::new();
        for i in 0..400 {
            rope.push_back(i.to_string());
        }

        rope.validate();
        dbg!(&rope.root);
        for i in 0..400 {
            let s = dbg!(rope.pop_front()).unwrap();
            dbg!(&rope.root);
            rope.validate();
            assert_eq!(s.parse::<u32>().unwrap(), i);
        }

        assert!(rope.is_empty());
    }

    #[test]
    fn pop_back() {
        let mut rope: Rope<String> = Rope::new();
        for i in 0..400 {
            rope.push_front(i.to_string());
        }

        rope.validate();
        dbg!(&rope.root);
        for i in 0..400 {
            let s = dbg!(rope.pop_back()).unwrap();
            dbg!(&rope.root);
            rope.validate();
            assert_eq!(s.parse::<u32>().unwrap(), i);
        }

        assert!(rope.is_empty());
    }

    #[test]
    fn first_last() {
        let mut rope: Rope<String> = Rope::new();
        for i in 0..200 {
            rope.push_back(i.to_string());
        }
        assert_eq!(rope.first().map(String::as_str), Some("0"));
        assert_eq!(rope.last().map(String::as_str), Some("199"));
    }

    #[test]
    fn iter() {
        let mut rope: Rope<String> = Rope::new();
        for i in 0..200 {
            rope.push_back(i.to_string());
        }

        dbg!(&rope.root);
        rope.validate();

        let elems: Vec<u32> = rope.iter().map(|x| x.parse().unwrap()).collect();
        assert_eq!(elems, (0..200).collect::<Vec<u32>>());

        let elems: Vec<u32> = rope.iter().rev().map(|x| x.parse().unwrap()).collect();
        assert_eq!(elems, (0..200).rev().collect::<Vec<u32>>());
    }

    #[test]
    fn range() {
        const COUNT: usize = ORDER * 4 + 7;

        let list: Vec<String> = (0..COUNT).map(|x| x.to_string()).collect();

        let rope: Rope<String> = list.iter().cloned().collect();
        dbg!(&rope.root);
        rope.validate();

        let len = rope.offset_len() as usize;
        // Create a table of the correct endpoint positions
        // for character indices `-1..=len + 1`
        let mut floor_idx = vec![0; len + 3];
        let mut ceil_idx = vec![0; len + 3];
        let mut off = 0;
        for (i, s) in rope.iter().enumerate() {
            floor_idx[1..][off..off + s.len()]
                .iter_mut()
                .for_each(|x| *x = i);
            ceil_idx[1..][off] = i;
            ceil_idx[1..][off + 1..off + s.len()]
                .iter_mut()
                .for_each(|x| *x = i + 1);
            off += s.len();
        }
        // i = `-1`
        floor_idx[0] = 0;
        ceil_idx[0] = 0;
        // i = `len`
        floor_idx[len + 1] = COUNT;
        ceil_idx[len + 1] = COUNT;
        // i = `len + 1`
        floor_idx[len + 2] = COUNT;
        ceil_idx[len + 2] = COUNT;

        // Positions of elements
        let mut off = 0;
        let mut off_table = vec![0];
        off_table.extend(rope.iter().map(|s| {
            off += s.len();
            off as isize
        }));

        // Try every possible range in a certain range
        for start in -1..=len as isize + 1 {
            for end in -1..=len as isize + 1 {
                for ty in 0..4 {
                    let (start_edge, start_expected_i) = if (ty & 1) != 0 {
                        (Edge::Floor(start as isize), floor_idx[(start + 1) as usize])
                    } else {
                        (Edge::Ceil(start as isize), ceil_idx[(start + 1) as usize])
                    };
                    let (end_edge, end_expected_i) = if (ty & 1) != 0 {
                        (Edge::Floor(end as isize), floor_idx[(end + 1) as usize])
                    } else {
                        (Edge::Ceil(end as isize), ceil_idx[(end + 1) as usize])
                    };

                    // If `end` < `start`, clamp `end`
                    let end_expected_i = std::cmp::max(start_expected_i, end_expected_i);

                    let range = start_edge..end_edge;
                    dbg!(&range);
                    let range = range_by_key(|o: &isize| *o, range);

                    let expected_list = if start_expected_i >= end_expected_i {
                        &[]
                    } else {
                        &list[start_expected_i..end_expected_i]
                    };

                    let (iter, offset_range) = rope.range(range.clone());
                    let elems: Vec<&String> = iter.collect();
                    let expected_elems: Vec<&String> = expected_list.iter().collect();
                    assert_eq!(elems, expected_elems);

                    let expected_offset_range =
                        off_table[start_expected_i]..off_table[end_expected_i];
                    assert_eq!(offset_range, expected_offset_range);

                    let (iter, _) = rope.range(range.clone());
                    let elems: Vec<&String> = iter.rev().collect();
                    let expected_elems: Vec<&String> = expected_list.iter().rev().collect();
                    assert_eq!(elems, expected_elems);
                }
            }
        }
    }

    #[test]
    fn get() {
        const COUNT: usize = ORDER * 4 + 7;

        let list: Vec<String> = (0..COUNT).map(|x| x.to_string()).collect();
        let total_len: usize = list.iter().map(|s| s.len()).sum();

        let rope: Rope<_> = list.iter().cloned().collect();

        assert_eq!(rope.get_with_offset(One::FirstAfter(by_ord(-1))), None);

        let mut i = 0;
        for e in list.iter() {
            let expected = Some((e, i as isize));
            assert_eq!(rope.get_with_offset(One::FirstAfter(by_ord(i))), expected);

            if e.len() > 1 {
                assert_eq!(
                    rope.get_with_offset(One::FirstAfter(by_ord(i + 1))),
                    expected
                );
                assert_eq!(
                    rope.get_with_offset(One::LastBefore(by_ord(i + 1))),
                    expected
                );

                assert_eq!(
                    rope.get_with_offset(One::FirstAfter(by_ord(i + e.len() as isize - 1))),
                    expected
                );
                assert_eq!(
                    rope.get_with_offset(One::LastBefore(by_ord(i + e.len() as isize - 1))),
                    expected
                );
            }

            assert_eq!(
                rope.get_with_offset(One::LastBefore(by_ord(i + e.len() as isize))),
                expected
            );

            i += e.len() as isize;
        }

        assert_eq!(
            rope.get_with_offset(One::FirstAfter(by_ord(total_len as isize))),
            None
        );

        assert_eq!(
            rope.get_with_offset(One::FirstAfter(by_ord(total_len as isize + 1))),
            None
        );
        assert_eq!(
            rope.get_with_offset(One::LastBefore(by_ord(total_len as isize + 1))),
            None
        );
    }

    #[test]
    fn remove() {
        const COUNT: usize = ORDER * 4 + 7;

        let list: Vec<String> = (0..COUNT).map(|x| x.to_string()).collect();

        for i in 0..list.len() {
            let mut rope: Rope<_, Index> = list.iter().cloned().collect();

            let one = One::FirstAfter(by_key(|key: &Index| key.0, i as isize));

            assert_eq!(rope.remove(one).unwrap().0, list[i],);

            rope.validate();

            // See if subsequent elements are shifted as expected
            for k in i + 1..list.len() {
                let one = One::FirstAfter(by_key(|key: &Index| key.0, k as isize - 1));
                assert_eq!(
                    rope.get_with_offset(one).unwrap(),
                    (&list[k], IndexOffset(k as isize - 1, NullOffset)),
                );
            }
        }
    }

    #[test]
    fn update_with() {
        const COUNT: usize = ORDER * 4 + 7;

        // All elements are one element long `Vec`s. So, their offset values
        // are like `Index`, until we change the length of one of them.
        let list: Vec<Vec<usize>> = (0..COUNT).map(|x| vec![x]).collect();

        for i in 0..list.len() {
            let mut rope: Rope<_> = list.iter().cloned().collect();

            let one = One::FirstAfter(by_ord(i as isize));

            // Change a `Vec`'s length to zero
            rope.update_with(one, |e, offset| {
                e.clear();
                assert_eq!(offset, i as isize);
            })
            .unwrap();

            rope.validate();

            // See if subsequent elements are shifted as expected
            for k in i + 1..list.len() {
                let one = One::FirstAfter(by_ord(k as isize - 1));
                assert_eq!(
                    rope.get_with_offset(one).unwrap(),
                    (&list[k], k as isize - 1),
                );
            }
        }
    }
}
