pub trait Iterutils: Iterator + Sized {
    /// Replace the element at the specified position.
    fn replace_at(self, index: usize, with: Self::Item) -> ReplaceAt<Self, Self::Item> {
        ReplaceAt {
            inner: self,
            count: index,
            with: Some(with),
        }
    }

    /// Like `filter_map`, but the function also receives the output index.
    fn filter_map_with_out_position<F, O>(self, filter: F) -> FilterMapWithOutPosition<Self, F>
    where
        F: FnMut(Self::Item, usize) -> Option<O>,
    {
        FilterMapWithOutPosition {
            inner: self,
            filter,
            i: 0,
        }
    }
}

impl<T: Iterator> Iterutils for T {}

#[derive(Clone)]
pub struct ReplaceAt<I, T> {
    inner: I,
    count: usize,
    with: Option<T>,
}

impl<I: Iterator> Iterator for ReplaceAt<I, I::Item> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|item| {
            if self.with.is_some() {
                if self.count == 0 {
                    return self.with.take().unwrap();
                } else {
                    self.count -= 1;
                }
            }
            item
        })
    }
}

#[derive(Clone)]
pub struct FilterMapWithOutPosition<I, F> {
    inner: I,
    filter: F,
    i: usize,
}

impl<I, F, O> Iterator for FilterMapWithOutPosition<I, F>
where
    I: Iterator,
    F: FnMut(I::Item, usize) -> Option<O>,
{
    type Item = O;

    fn next(&mut self) -> Option<O> {
        while let Some(inp) = self.inner.next() {
            if let Some(out) = (self.filter)(inp, self.i) {
                self.i += 1;
                return Some(out);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.inner.size_hint().1)
    }
}

impl<I, F> std::iter::FusedIterator for FilterMapWithOutPosition<I, F>
where
    I: std::iter::FusedIterator,
    F: FnMut(I::Item, usize) -> Option<I::Item>,
{
}
