//! Provides a customized version of `std::collections::LinkedList`.
#![feature(box_into_raw_non_null)]
pub mod linked_list;
pub mod cell;

pub use crate::{linked_list::{Iter, IterMut, LinkedList}, cell::LinkedListCell};
