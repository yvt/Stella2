//! Provides an iterator type that outputs the difference between two sorted
//! sequences.
//!
//! It's implemented using an algorithm similar to merge sort and the time
//! complexity is O(N) (where N is the number of elements).
//!
//! The result is unspecified if the input is unsorted.
//!
//! # Examples
//!
//!     use sorted_diff::{sorted_diff, In};
//!
//!     let v1 = vec![   2, 5, 6, 7,    9];
//!     let v2 = vec![1, 2, 5, 6,    8   ];
//!
//!     assert_eq!(
//!         sorted_diff(v1, v2).collect::<Vec<_>>(),
//!         vec![
//!             In::Right(1),
//!             In::Both(2, 2),
//!             In::Both(5, 5),
//!             In::Both(6, 6),
//!             In::Left(7),
//!             In::Right(8),
//!             In::Left(9),
//!         ],
//!     );
//!
use std::{
    cmp::Ordering,
    iter::{FusedIterator, Peekable},
};

/// An iterator that outputs the difference between two sorted sequences.
/// Elements are compared using the [comparer](Cmp) `C`.
pub struct SortedDiffBy<I1: Iterator, I2: Iterator, C> {
    it1: Peekable<I1>,
    it2: Peekable<I2>,
    cmp: C,
}

/// An element included in either or both of two sequences.
/// This type is produced by [`SortedDiffBy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum In<T1, T2> {
    /// An element included only in the first sequence.
    Left(T1),
    /// An element included only in the second sequence.
    Right(T2),
    /// Two elements equivalent to each other, included in their respective
    /// sequences.
    Both(T1, T2),
}

impl<I1, I2, C> Iterator for SortedDiffBy<I1, I2, C>
where
    I1: Iterator,
    I2: Iterator,
    C: Cmp<I1::Item, I2::Item>,
{
    type Item = In<I1::Item, I2::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.it1.peek(), self.it2.peek()) {
            (None, None) => None,
            (Some(_), None) => Some(In::Left(self.it1.next().unwrap())),
            (None, Some(_)) => Some(In::Right(self.it2.next().unwrap())),
            (Some(x1), Some(x2)) => match self.cmp.cmp(x1, x2) {
                Ordering::Less => Some(In::Left(self.it1.next().unwrap())),
                Ordering::Greater => Some(In::Right(self.it2.next().unwrap())),
                Ordering::Equal => {
                    Some(In::Both(self.it1.next().unwrap(), self.it2.next().unwrap()))
                }
            },
        }
    }
}

impl<I1, I2, C> FusedIterator for SortedDiffBy<I1, I2, C>
where
    I1: Iterator + FusedIterator,
    I2: Iterator + FusedIterator,
    C: Cmp<I1::Item, I2::Item>,
{
}

impl<I1, I2, C> SortedDiffBy<I1, I2, C>
where
    I1: Iterator,
    I2: Iterator,
    C: Cmp<I1::Item, I2::Item>,
{
    /// Construct a `SortedDiffBy` using a specified comparer.
    ///
    /// This is a low-level function of [`sorted_diff_by`].
    pub fn new_by(it1: I1, it2: I2, cmp: C) -> Self {
        Self {
            it1: it1.peekable(),
            it2: it2.peekable(),
            cmp,
        }
    }
}

/// Compares two objects.
pub trait Cmp<T1, T2> {
    fn cmp(&mut self, obj1: &T1, obj2: &T2) -> Ordering;
}

impl<F, T1, T2> Cmp<T1, T2> for F
where
    F: FnMut(&T1, &T2) -> Ordering,
{
    fn cmp(&mut self, obj1: &T1, obj2: &T2) -> Ordering {
        self(obj1, obj2)
    }
}

/// A [`Cmp`] implementation that uses the `Ord` implementation of the target
/// type.
///
/// This trait allows [`SortedDiffBy`] to generalize for both of custom and
/// default comparers.
#[derive(Debug, Clone, Copy)]
pub struct DefaultCmp;

impl<T> Cmp<T, T> for DefaultCmp
where
    T: Ord,
{
    fn cmp(&mut self, obj1: &T, obj2: &T) -> Ordering {
        obj1.cmp(obj2)
    }
}

// ------------------------------------------------------------------------
//  Variations

/// An iterator that outputs the difference between two sorted sequences.
/// Elements are compared using `<I1::Item as Ord>::cmp`.
pub type SortedDiff<I1, I2> = SortedDiffBy<I1, I2, DefaultCmp>;

impl<I1, I2> SortedDiff<I1, I2>
where
    I1: Iterator,
    I2: Iterator,
{
    /// Construct a `SortedDiffBy` using `Ord::cmp` for comparison.
    ///
    /// This is a low-level function of [`sorted_diff`].
    pub fn new(it1: I1, it2: I2) -> Self {
        Self {
            it1: it1.peekable(),
            it2: it2.peekable(),
            cmp: DefaultCmp,
        }
    }
}

// ------------------------------------------------------------------------
//  Constructors

/// Consturct a `SortedDiff` using the specified two `IntoIterator`s as the
/// input sequences.
pub fn sorted_diff<I1, I2>(it1: I1, it2: I2) -> SortedDiff<I1::IntoIter, I2::IntoIter>
where
    I1: IntoIterator,
    I2: IntoIterator<Item = I1::Item>,
    I1::Item: Ord,
{
    SortedDiff::new(it1.into_iter(), it2.into_iter())
}

/// Consturct a `SortedDiff` using the specified two `IntoIterator`s as the
/// input sequences, and a custom comparer.
///
/// `cmp` is usually `impl FnMut(&I1::Item, &I2::Item) -> std::cmp::Ordering`,
/// but other types implementing [`Cmp`]`<I1::Item, I2::Item>` can be used, too.
pub fn sorted_diff_by<I1, I2, C>(
    it1: I1,
    it2: I2,
    cmp: C,
) -> SortedDiffBy<I1::IntoIter, I2::IntoIter, C>
where
    I1: IntoIterator,
    I2: IntoIterator,
    C: Cmp<I1::Item, I2::Item>,
{
    SortedDiffBy::new_by(it1.into_iter(), it2.into_iter(), cmp)
}
