//! Intrusive doubly linked list for `Pool` and `IterablePool`.
use crate::PoolPtr;

/// Specialization of `ListHead` for `Pool` and `IterablePool`.
pub type ListHead = array_intrusive_list::ListHead<PoolPtr>;
/// Specialization of `Link` for `Pool` and `IterablePool`.
pub type Link = array_intrusive_list::Link<PoolPtr>;
#[doc(no_inline)]
pub use array_intrusive_list::ListAccessorCell;
