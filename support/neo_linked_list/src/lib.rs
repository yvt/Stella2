//! Provides a customized version of `std::collections::LinkedList`.
pub mod cell;
pub mod linked_list;

pub use crate::{
    cell::LinkedListCell,
    linked_list::{Iter, IterMut, LinkedList},
};

/// Implements `Unpin` regardless of whether the inner type has it or not.
///
/// Useful for storing types that do not implement `Unpin` but are never used
/// with `Pin`, in `LinkedList`.
///
/// # Examples
///
/// ```
/// use neo_linked_list::{LinkedList, AssertUnpin, linked_list::Node};
///
/// let mut d = LinkedList::<AssertUnpin<dyn Fn() -> u32>>::new();
/// d.push_back_node(Node::pin(AssertUnpin::new(|| 42)));
/// assert_eq!(42, (d.back().unwrap().inner)());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssertUnpin<T: ?Sized> {
    pub inner: T,
}

impl<T: ?Sized> Unpin for AssertUnpin<T> {}

impl<T> AssertUnpin<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}
