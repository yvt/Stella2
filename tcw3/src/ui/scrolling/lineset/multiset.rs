const ELEM_MAX: u8 = 64;

/// A multiset for integers in range `0..ELEM_MAX`. It can contain up to 255
/// elements (limited by `u8`). And it supports querying the minimum element.
///
/// All operations are O(1).
pub struct Minimultiset {
    counts: [u8; ELEM_MAX as usize],
    /// Each bit indicates whether the corresponding element in `counts` is
    /// non-zero or not. Must have `ELEM_MAX` bits.
    nonzero: u64,
}

impl Minimultiset {
    pub fn new() -> Self {
        Self {
            counts: [0; ELEM_MAX as usize],
            nonzero: 0,
        }
    }

    /// Insert an element.
    ///
    /// The behavior is unspecified if the element count overflows.
    pub fn insert(&mut self, x: u8) {
        assert!(x < ELEM_MAX);
        self.counts[x as usize] += 1;
        self.nonzero |= 1 << x;
    }

    /// Remove an element.
    ///
    /// The behavior is unspecified if the element does not exist.
    pub fn remove(&mut self, x: u8) {
        assert!(x < ELEM_MAX);
        self.counts[x as usize] -= 1;
        if self.counts[x as usize] == 0 {
            self.nonzero &= !(1u64 << x);
        }
    }

    /// Get the minimum element.
    ///
    /// The behavior is unspecified if the multiset is empty.
    pub fn min(&mut self) -> u8 {
        debug_assert_ne!(self.nonzero, 0);
        self.nonzero.trailing_zeros() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut set = Minimultiset::new();
        set.insert(3);
        assert_eq!(set.min(), 3);
        set.insert(3);
        assert_eq!(set.min(), 3);
        set.insert(7);
        assert_eq!(set.min(), 3);
        set.remove(3);
        assert_eq!(set.min(), 3);
        set.remove(3);
        assert_eq!(set.min(), 7);
        set.insert(5);
        assert_eq!(set.min(), 5);
    }
}
