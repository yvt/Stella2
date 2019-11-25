//! Provides a customized version of `std::collections::LinkedList`.
#![feature(box_into_raw_non_null)]
pub mod linked_list;

pub use crate::linked_list::{LinkedList, Iter, IterMut};
