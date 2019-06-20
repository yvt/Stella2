//! Miscellaneous/non-essential/debug definitions
use std::{fmt, iter::FromIterator};

use super::{NodeRef, Offset, Rope, ToOffset, ORDER};

impl<T, O> Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    /// Check the integrity of the rope structure. Panics if the integrity
    /// check fails.
    #[doc(hidden)]
    pub fn validate(&self)
    where
        O: PartialEq + fmt::Debug,
    {
        let len = Self::validate_sub(&self.root, true);
        assert_eq!(len, self.len);
    }

    /// The internal method for `validate`. Returns the actual length of
    /// the node.
    fn validate_sub(node: &NodeRef<T, O>, is_root: bool) -> O
    where
        O: PartialEq + fmt::Debug,
    {
        match node {
            NodeRef::Internal(inode) => {
                let min_count = if is_root { 2 } else { ORDER };
                assert!(
                    inode.children.len() >= min_count,
                    "bad child count: {} (min: {})",
                    inode.children.len(),
                    min_count
                );
                assert_eq!(inode.offsets.len(), inode.children.len() - 1);

                // Check the child types
                assert!(
                    inode.children.iter().all(NodeRef::is_leaf)
                        || inode.children.iter().all(NodeRef::is_internal),
                    "child type constraint violation"
                );

                // Check the offsets
                let mut offset = O::zero();

                for i in 0..inode.offsets.len() {
                    offset += Self::validate_sub(&inode.children[i], false);
                    assert_eq!(
                        offset, inode.offsets[i],
                        "offset ({:?}) != inode.offsets[{:?}] ({:?})",
                        offset, i, inode.offsets[i],
                    );
                }

                let i = inode.offsets.len();
                offset += Self::validate_sub(&inode.children[i], false);

                offset
            }
            NodeRef::Leaf(elements) => {
                let min_count = if is_root { 0 } else { ORDER };
                assert!(
                    elements.len() >= min_count,
                    "bad element count: {} (min: {})",
                    elements.len(),
                    min_count
                );

                elements
                    .iter()
                    .map(ToOffset::to_offset)
                    .fold(O::zero(), |x, y| x + y)
            }
            NodeRef::Invalid => unreachable!(),
        }
    } // fn validate_sub
}

impl<T, O> FromIterator<T> for Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

impl<T, O> Extend<T> for Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        // We could do bulk load, but for now take a naïve approach
        for e in iter {
            self.push_back(e);
        }
    }
}

impl<T, O> fmt::Debug for Rope<T, O>
where
    T: ToOffset<O> + fmt::Debug,
    O: Offset,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
