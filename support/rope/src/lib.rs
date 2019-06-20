//! This crate implements a variant of [the rope data structure] inspired from
//! [the B+ tree].
//!
//! [the rope data structure]: https://en.m.wikipedia.org/wiki/Rope_(data_structure)
//! [the B+ tree]: https://en.wikipedia.org/wiki/B+_tree
//!
//! Logically, it can be modeled as a sequence of elements, each having a value
//! representing the length of type implementing [`Offset`] (calculated by
//! `<T as ToOffset<O>>::to_offset`).
//! It supports the following operations:
//!
//!  - O(log n) insertion at an arbitrary location.
//!  - O(log n) removal of an arbitrary location.
//!  - O(log n) search by an offset value relative to the start or end of the
//!    sequence.
//!
//! It does not support indexing like normal arrays. However, it can be added
//! by combining an existing `Offset` with [`IndexOffset`].
use arrayvec::ArrayVec;
use std::ops::{Add, AddAssign, Neg};

mod iter;
mod misc;
mod ops;
pub use self::iter::*;

/// Represents a rope.
///
/// See [the crate documentation](index.html) for more.
#[derive(Clone)]
pub struct Rope<T, O = isize> {
    root: NodeRef<T, O>,
    len: O,
}

/// A trait for types whose length can be calculated.
pub trait ToOffset<O: Offset> {
    fn to_offset(&self) -> O;
}

impl<T> ToOffset<isize> for Vec<T> {
    fn to_offset(&self) -> isize {
        assert!(self.len() <= <isize>::max_value() as usize, "len overflow");
        self.len() as isize
    }
}

impl ToOffset<isize> for String {
    fn to_offset(&self) -> isize {
        assert!(self.len() <= <isize>::max_value() as usize, "len overflow");
        self.len() as isize
    }
}

/// A trait for offset values.
///
/// Additive operations on offset values must be exact.
pub trait Offset: Neg<Output = Self> + Add<Output = Self> + AddAssign + Sized + Clone {
    fn zero() -> Self;
}

impl Offset for isize {
    fn zero() -> isize {
        0
    }
}

/// [`Offset`] having no information.
///
/// `ToOffset<NullOffset>` is automatically implemented for all types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NullOffset;

impl Add for NullOffset {
    type Output = Self;
    fn add(self, _: Self) -> Self {
        self
    }
}

impl AddAssign for NullOffset {
    fn add_assign(&mut self, _: Self) {}
}

impl Neg for NullOffset {
    type Output = Self;
    fn neg(self) -> Self {
        self
    }
}

impl Offset for NullOffset {
    fn zero() -> Self {
        NullOffset
    }
}

impl<T> ToOffset<NullOffset> for T {
    fn to_offset(&self) -> NullOffset {
        NullOffset
    }
}

/// Adds an element index to another [`Offset`].
///
/// [`ToOffset`]`<IndexOffset<T>>` is automatically implemented for types
/// that implement `ToOffset<T>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOffset<T>(pub isize, pub T);

impl<T> Add for IndexOffset<T>
where
    T: Add<T, Output = T>,
{
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        IndexOffset(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl<T> AddAssign for IndexOffset<T>
where
    T: AddAssign,
{
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
        self.1 += rhs.1;
    }
}

impl<T> Neg for IndexOffset<T>
where
    T: Neg<Output = T>,
{
    type Output = Self;
    fn neg(self) -> Self {
        IndexOffset(-self.0, -self.1)
    }
}

impl<T> Offset for IndexOffset<T>
where
    T: Add<T, Output = T> + Neg<Output = T> + Offset,
{
    fn zero() -> Self {
        IndexOffset(0, T::zero())
    }
}

impl<T, O> ToOffset<IndexOffset<O>> for T
where
    T: ToOffset<O>,
    O: Offset,
{
    fn to_offset(&self) -> IndexOffset<O> {
        IndexOffset(1, ToOffset::<O>::to_offset(self))
    }
}

/// `Offset` representing an element index.
pub type Index = IndexOffset<NullOffset>;

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

#[derive(Debug, Default)]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let rope: Rope<String> = Rope::new();
        dbg!(&rope.root);
        rope.validate();
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
}
