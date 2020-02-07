use cggeom::{prelude::*, Box2};
use packed_simd::usizex2;

/// Efficiently computes the union of a set of `Box2<usize>`s.
///
/// TODO: Move this to `cggeom`
#[derive(Debug, Clone, Copy)]
pub struct Box2UsizeUnion {
    // Optimize for x86_64 + SSE2. Each XMM register can contain two `usize`s.
    // Since SSE can execute only one of min and max in a single instruction
    // anyway, this layout suits well. (min can be converted to max by flipping
    // bits, but that will do more harm than good without native `usizex4`-sized
    // vector registers)
    min: usizex2,
    max: usizex2,
}

impl Box2UsizeUnion {
    #[inline]
    pub fn new() -> Self {
        Self {
            min: [usize::max_value(); 2].into(),
            max: [usize::min_value(); 2].into(),
        }
    }

    /// Insert a box `x` to the input set.
    ///
    /// If `x` is `None`, the set is not modified. If `x` is `Some(bx)`, `bx`
    /// is inserted to the set. In this case, `bx` must not be empty.
    #[inline]
    pub fn insert(&mut self, x: Option<Box2<usize>>) {
        if let Some(x) = x {
            debug_assert!(!x.is_empty());
            let [min, max]: [[usize; 2]; 2] = [x.min.into(), x.max.into()];
            self.min = self.min.min(min.into());
            self.max = self.max.max(max.into());
        }
    }

    /// Get the union. Returns `None` is the result is empty.
    #[inline]
    pub fn into_box2(&self) -> Option<Box2<usize>> {
        let [min, max]: [[usize; 2]; 2] = [self.min.into(), self.max.into()];
        let bx = Box2::new(min.into(), max.into());
        if self.min.ge(self.max).any() {
            None
        } else {
            Some(bx)
        }
    }
}

impl Extend<Option<Box2<usize>>> for Box2UsizeUnion {
    fn extend<T: IntoIterator<Item = Option<Box2<usize>>>>(&mut self, iter: T) {
        for x in iter {
            self.insert(x);
        }
    }
}

impl Extend<Box2<usize>> for Box2UsizeUnion {
    fn extend<T: IntoIterator<Item = Box2<usize>>>(&mut self, iter: T) {
        for x in iter {
            self.insert(Some(x));
        }
    }
}

impl std::iter::FromIterator<Option<Box2<usize>>> for Box2UsizeUnion {
    fn from_iter<T: IntoIterator<Item = Option<Box2<usize>>>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

impl std::iter::FromIterator<Box2<usize>> for Box2UsizeUnion {
    fn from_iter<T: IntoIterator<Item = Box2<usize>>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cggeom::box2;
    use quickcheck::TestResult;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn test_box2_usize_union(coords: Vec<usize>) -> TestResult {
        let boxes = coords.chunks_exact(4).map(|coords| {
            let bx = box2! { min: [coords[0], coords[1]], max: [coords[2], coords[3]] };
            if bx.is_empty() { None } else { Some(bx) }
        });

        let expected = boxes
            .clone()
            .fold(None, |x: Option<Box2<usize>>, y| match (x, y) {
                (Some(x), Some(y)) => Some(x.union(&y)),
                (x, None) => x,
                (None, y) => y,
            });

        let actual = boxes.collect::<Box2UsizeUnion>().into_box2();

        if expected != actual {
            return TestResult::error(format!("expected = {:?}, got = {:?}", expected, actual));
        }

        TestResult::passed()
    }
}
