pub trait Iterutils: Iterator + Sized {
    /// Replace the element at the specified position.
    fn replace_at(self, index: usize, with: Self::Item) -> ReplaceAt<Self, Self::Item> {
        ReplaceAt {
            inner: self,
            count: index,
            with: Some(with),
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
