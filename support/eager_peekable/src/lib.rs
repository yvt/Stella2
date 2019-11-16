//! Like `Iterator::peekable`, but advances the original iterator more eagerly
//! for potential execution efficiency.

/// An extension trait for `Iterator` that provides a method `eager_peekable`
/// that behaves similarly to `peekable` but advances the original iterator more
/// eagerly.
pub trait EagerPeekableExt: Sized + Iterator {
    /// Create an iterator which can use `peek` to look at the next element of
    /// the iterator without consuming it.
    ///
    /// Unlike `Iterator::peekable()`, the original iterator is advanced even
    /// if `peek` is not called.
    ///
    /// # Examples
    ///
    /// The following example code was copied from `Iterator::peekable`'s
    /// documentation:
    ///
    /// ```
    /// use eager_peekable::EagerPeekableExt;
    /// let xs = [1, 2, 3];
    ///
    /// let mut iter = xs.iter().eager_peekable();
    ///
    /// // peek() lets us see into the future
    /// assert_eq!(iter.peek(), Some(&&1));
    /// assert_eq!(iter.next(), Some(&1));
    ///
    /// assert_eq!(iter.next(), Some(&2));
    ///
    /// // we can peek() multiple times, the iterator won't advance
    /// assert_eq!(iter.peek(), Some(&&3));
    /// assert_eq!(iter.peek(), Some(&&3));
    ///
    /// assert_eq!(iter.next(), Some(&3));
    ///
    /// // after the iterator is finished, so is peek()
    /// assert_eq!(iter.peek(), None);
    /// assert_eq!(iter.next(), None);
    /// ```
    ///
    /// `eager_peekable`-specific:
    ///
    /// ```
    /// # use eager_peekable::EagerPeekableExt;
    /// # let xs = [1, 2, 3];
    /// let mut iter_inner = xs.iter();
    ///
    /// // Combine `eager_peekable` without consuming the original iterator
    /// iter_inner.by_ref().eager_peekable();
    ///
    /// assert_eq!(iter_inner.next(), Some(&2));
    /// ```
    fn eager_peekable(mut self) -> EagerPeekable<Self> {
        EagerPeekable {
            peeked: self.next(),
            iter: self,
        }
    }
}

impl<T: Iterator> EagerPeekableExt for T {}

/// Like `std::iter::Peekable`, but advances the original iterator more eagerly
/// for potential execution efficiency.
#[must_use = "iterators are lazy and do nothing unless consumed"]
#[derive(Debug, Clone)]
pub struct EagerPeekable<I: Iterator> {
    iter: I,
    peeked: Option<I::Item>,
}

impl<I: Iterator> EagerPeekable<I> {
    #[inline]
    pub fn peek(&self) -> Option<&I::Item> {
        self.peeked.as_ref()
    }
}

impl<I: Iterator> Iterator for EagerPeekable<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        std::mem::replace(&mut self.peeked, self.iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.iter.size_hint();
        let num_peeked = self.peeked.is_some() as usize;
        (lower, upper.and_then(|i| i.checked_add(num_peeked)))
    }

    fn count(self) -> usize {
        let num_peeked = self.peeked.is_some() as usize;
        drop(self.peeked);

        self.iter.count() + num_peeked
    }
}

impl<I: std::iter::FusedIterator> std::iter::FusedIterator for EagerPeekable<I> {}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
