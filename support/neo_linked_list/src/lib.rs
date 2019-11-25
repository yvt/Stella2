//! Provides a customized version of `std::collections::LinkedList`.
#![feature(box_into_raw_non_null)]
pub mod cell;
pub mod linked_list;

pub use crate::{
    cell::LinkedListCell,
    linked_list::{Iter, IterMut, LinkedList},
};
