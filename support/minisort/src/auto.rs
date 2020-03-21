//! Provides a sort implementation that chooses between `cstdlib` and
//! `insertion` depending on the element count.
use crate::{cstdlib, insertion};
use std::cmp::Ordering;

const THRESHOLD: usize = 128;

/// Sort the slice using [`insertion_sort`] or [`qsort`] depending on the
/// element count.
///
/// [`insertion_sort`]: crate::insertion_sort
/// [`qsort`]: crate::qsort
///
/// For a small input, this function uses `insertion_sort` for performance and
/// a small code size. For a large input, it switches to `qsort` to avoid the
/// catastrophic quadratic performance of `insertion_sort` without increasing
/// the code size significantly.
///
/// # Examples
///
/// ```
/// let mut v = [-5, 4, 1, -3, 2];
///
/// minisort::minisort(&mut v);
/// assert!(v == [-5, -3, 1, 2, 4]);
/// ```
pub fn minisort<T: Ord>(a: &mut [T]) {
    if a.len() >= THRESHOLD {
        cstdlib::qsort(a);
    } else {
        insertion::insertion_sort(a);
    }
}

/// Sort the slice with a key extraction function. See [`minisort`] and
/// [`qsort_by_key`](crate::qsort_by_key).
///
/// # Examples
///
/// ```
/// let mut v = [-5i32, 4, 1, -3, 2];
///
/// minisort::minisort_by_key(&mut v, |k| k.abs());
/// assert!(v == [1, 2, -3, 4, -5]);
/// ```
pub fn minisort_by_key<T, K: Ord>(a: &mut [T], f: impl FnMut(&T) -> K) {
    if a.len() >= THRESHOLD {
        cstdlib::qsort_by_key(a, f);
    } else {
        insertion::insertion_sort_by_key(a, f);
    }
}

/// Sort the slice with a comparator function. See [`minisort`] and
/// [`qsort_by`](crate::qsort_by).
///
/// # Examples
///
/// ```
/// let mut v = [5, 4, 1, 3, 2];
/// minisort::minisort_by(&mut v, |a, b| a.cmp(b));
/// assert!(v == [1, 2, 3, 4, 5]);
/// ```
pub fn minisort_by<T>(a: &mut [T], f: impl FnMut(&T, &T) -> Ordering) {
    if a.len() >= THRESHOLD {
        cstdlib::qsort_by(a, f);
    } else {
        insertion::insertion_sort_by(a, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn result_is_sorted(mut v: Vec<i32>) -> bool {
        minisort(&mut v);
        v.is_sorted()
    }

    #[quickcheck]
    fn result_is_sorted_by_key(mut v: Vec<(i32, i32)>) -> bool {
        minisort_by_key(&mut v, |e| e.1);
        v.is_sorted_by_key(|e| e.1)
    }

    #[quickcheck]
    fn result_is_sorted_by(mut v: Vec<i32>) -> bool {
        minisort_by(&mut v, |x, y| y.cmp(x));
        v.is_sorted_by(|x, y| Some(y.cmp(x)))
    }
}
