use std::{borrow::Borrow, marker::PhantomData};

/// Resettable iterator.
///
/// `ResetIter` does not include `Iterator` as its subtrait because the current
/// implementation of the Rust compiler generates vtable entries even for
/// methods (there are 64 of them at the point of writing) with a
/// `where Self: Sized` constraint.
pub trait ResetIter {
    type Item;

    /// Reset the iterator.
    fn reset(&mut self);

    /// Advance the iterator and return the next value.
    fn next(&mut self) -> Option<Self::Item>;
}

pub struct Empty<T>(PhantomData<T>);

impl<T> ResetIter for Empty<T> {
    type Item = T;
    fn reset(&mut self) {}
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

/// Constructs a `ResetIter` producing no elements.
pub fn empty<T>() -> Empty<T> {
    Empty(PhantomData)
}

impl<T: ResetIter + ?Sized> ResetIter for &mut T {
    type Item = T::Item;
    fn reset(&mut self) {
        (**self).reset();
    }
    fn next(&mut self) -> Option<Self::Item> {
        (**self).next()
    }
}

impl<T: ResetIter + ?Sized> ResetIter for Box<T> {
    type Item = T::Item;
    fn reset(&mut self) {
        (**self).reset();
    }
    fn next(&mut self) -> Option<Self::Item> {
        (**self).next()
    }
}

/// Conversion into a [`ResetIter`].
///
/// # Examples
///
/// ```
/// use tcw3::prelude::*;
///
/// // &'static [u32]
/// let mut reset_iter = (&[1, 2, 3][..]).into_reset_iter();
/// assert_eq!(reset_iter.collect::<Vec<u32>>(), vec![1, 2, 3]);
///
/// // Vec<u32>
/// let mut reset_iter = vec![1, 2, 3].into_reset_iter();
/// assert_eq!(reset_iter.collect::<Vec<u32>>(), vec![1, 2, 3]);
///
/// // RangeInclusive<u32>
/// let mut reset_iter = (1u32..=3).into_reset_iter();
/// assert_eq!(reset_iter.collect::<Vec<u32>>(), vec![1, 2, 3]);
/// ```
pub trait IntoResetIter {
    type Item;
    type IntoResetIter: ResetIter<Item = Self::Item>;
    fn into_reset_iter(self) -> Self::IntoResetIter;
}

impl<T: ResetIter> IntoResetIter for T {
    type Item = T::Item;
    type IntoResetIter = Self;

    fn into_reset_iter(self) -> Self::IntoResetIter {
        self
    }
}

impl<T> IntoResetIter for std::ops::Range<T>
where
    Self: Iterator + Clone,
    T: Clone,
{
    type Item = <Self as Iterator>::Item;
    type IntoResetIter = RangeIter<Self, T>;

    fn into_reset_iter(self) -> Self::IntoResetIter {
        RangeIter {
            start: self.start.clone(),
            iter: self,
        }
    }
}

impl<T> IntoResetIter for std::ops::RangeInclusive<T>
where
    Self: Iterator + Clone,
    T: Clone,
{
    type Item = <Self as Iterator>::Item;
    type IntoResetIter = RangeIter<Self, T>;

    fn into_reset_iter(self) -> Self::IntoResetIter {
        RangeIter {
            start: self.start().clone(),
            iter: self,
        }
    }
}

impl<T> IntoResetIter for std::ops::RangeFrom<T>
where
    Self: Iterator + Clone,
    T: Clone,
{
    type Item = <Self as Iterator>::Item;
    type IntoResetIter = RangeIter<Self, T>;

    fn into_reset_iter(self) -> Self::IntoResetIter {
        RangeIter {
            start: self.start.clone(),
            iter: self,
        }
    }
}

/// Wraps `Range` to use it as [`ResetIter`].
pub struct RangeIter<Iter, T> {
    iter: Iter,
    start: T,
}

impl<T> ResetIter for RangeIter<std::ops::Range<T>, T>
where
    std::ops::Range<T>: Iterator + Clone,
    T: Clone,
{
    type Item = <std::ops::Range<T> as Iterator>::Item;

    fn reset(&mut self) {
        self.iter.start = self.start.clone();
    }

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T> ResetIter for RangeIter<std::ops::RangeInclusive<T>, T>
where
    std::ops::RangeInclusive<T>: Iterator + Clone,
    T: Clone,
{
    type Item = <std::ops::RangeInclusive<T> as Iterator>::Item;

    fn reset(&mut self) {
        self.iter = self.start.clone()..=self.iter.end().clone();
    }

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<T> ResetIter for RangeIter<std::ops::RangeFrom<T>, T>
where
    std::ops::RangeFrom<T>: Iterator + Clone,
    T: Clone,
{
    type Item = <std::ops::RangeFrom<T> as Iterator>::Item;

    fn reset(&mut self) {
        self.iter.start = self.start.clone();
    }

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<Item> IntoResetIter for &'_ [Item]
where
    Item: Clone,
{
    type Item = Item;
    type IntoResetIter = SliceIter<Self, Item>;

    fn into_reset_iter(self) -> Self::IntoResetIter {
        SliceIter {
            slice: self,
            i: 0,
            item: PhantomData,
        }
    }
}

impl<Item> IntoResetIter for Vec<Item>
where
    Item: Clone,
{
    type Item = Item;
    type IntoResetIter = SliceIter<Self, Item>;

    fn into_reset_iter(self) -> Self::IntoResetIter {
        SliceIter {
            slice: self,
            i: 0,
            item: PhantomData,
        }
    }
}

pub struct SliceIter<Slice, Item> {
    i: usize,
    slice: Slice,
    item: PhantomData<Item>,
}

impl<Slice, Item> ResetIter for SliceIter<Slice, Item>
where
    Slice: Borrow<[Item]>,
    Item: Clone,
{
    type Item = Item;

    fn reset(&mut self) {
        self.i = 0;
    }

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(x) = self.slice.borrow().get(self.i) {
            self.i += 1;
            Some(x.clone())
        } else {
            None
        }
    }
}

/// An extension trait for [`ResetIter`].
pub trait ResetIterExt: ResetIter {
    fn is_empty(&mut self) -> bool {
        self.reset();
        self.next().is_none()
    }

    fn iter(&mut self) -> ResetIterIter<'_, Self> {
        self.reset();
        ResetIterIter { inner: self }
    }

    /// Map elements using the specified function.
    ///
    /// # Examples
    ///
    /// ```
    /// use tcw3::prelude::*;
    /// let a = [1u32, 2, 3];
    ///
    /// let mut iter = a.into_reset_iter().map(|x| 2 * x);
    ///
    /// assert_eq!(iter.next(), Some(2));
    /// assert_eq!(iter.next(), Some(4));
    /// assert_eq!(iter.next(), Some(6));
    /// ```
    fn map<Transducer, MapTo>(self, transducer: Transducer) -> Map<Self, Transducer>
    where
        Transducer: FnMut(Self::Item) -> MapTo,
        Self: Sized,
    {
        Map {
            iter: self,
            transducer,
        }
    }

    /// Transform the iterator into a collection.
    ///
    /// Unlike `Iterator::collect`, this method does not consume the iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use tcw3::prelude::*;
    /// let a = [1, 2, 3];
    /// let mut iter = a.into_reset_iter().map(|x| x * 2);
    ///
    /// let doubled1: Vec<i32> = iter.collect();
    /// let doubled2: Vec<i32> = iter.collect();
    ///
    /// assert_eq!(vec![2, 4, 6], doubled1);
    /// assert_eq!(vec![2, 4, 6], doubled2);
    /// ```
    fn collect<To>(&mut self) -> To
    where
        To: std::iter::FromIterator<Self::Item>,
    {
        self.iter().collect()
    }
}

impl<T: ResetIter + ?Sized> ResetIterExt for T {}

/// The return value of [`ResetIterExt::iter`].
pub struct ResetIterIter<'a, T: ?Sized> {
    inner: &'a mut T,
}

impl<T: ResetIter + ?Sized> Iterator for ResetIterIter<'_, T> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// The return value of [`ResetIterExt::map`].
pub struct Map<Iter, Transducer> {
    iter: Iter,
    transducer: Transducer,
}

impl<Iter, Transducer, MapTo> ResetIter for Map<Iter, Transducer>
where
    Iter: ResetIter,
    Transducer: FnMut(Iter::Item) -> MapTo,
{
    type Item = MapTo;

    fn reset(&mut self) {
        self.iter.reset()
    }
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(&mut self.transducer)
    }
}
