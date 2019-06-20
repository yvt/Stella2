//! Iterators for `Rope`
use arrayvec::ArrayVec;
use std::iter::FusedIterator;

use super::{Cursor, NodeRef, Offset, Rope, ToOffset, CURSOR_LEN};

impl<T, O> Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    fn iter_with_cursor(&self) -> IterWithCursor<'_, T, O> {
        let mut cursor = IterWithCursor {
            path: ArrayVec::new(),
            indices: Default::default(),
        };
        let mut cur = &self.root;

        loop {
            match cur {
                NodeRef::Internal(inode) => {
                    cursor.path.push(cur);
                    cur = &inode.children[0];
                }
                NodeRef::Leaf(leaf) => {
                    if leaf.is_empty() {
                        // The rope is empty. Empty leaf nodes are allowed
                        // only as a root.
                        debug_assert!(cursor.path.is_empty());
                    } else {
                        cursor.path.push(cur);
                    }
                    break;
                }
                NodeRef::Invalid => unreachable!(),
            }
        }

        cursor
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a {
        self.iter_with_cursor().map(|(_, item)| item)
    }
}

#[derive(Debug)]
struct IterWithCursor<'a, T, O> {
    /// The parent `NodeRef` for each index in `indices`.
    path: ArrayVec<[&'a NodeRef<T, O>; CURSOR_LEN]>,
    /// The same as `Cursor::indices`, but does not have length information.
    /// However, this must not reference the one-past-end element.
    indices: [u8; CURSOR_LEN],
}

impl<'a, T, O> Iterator for IterWithCursor<'a, T, O> {
    type Item = (Cursor, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.path.len() == 0 {
            return None;
        }

        // Convert the internal pointer to `Cursor`
        let mut cursor = Cursor {
            indices: ArrayVec::from(self.indices),
            _pad: Default::default(),
        };
        cursor.indices.truncate(self.path.len());

        // Get the current element
        let level = self.path.len() - 1;
        let (elem, exhausted) = match self.path[level] {
            NodeRef::Leaf(leaf) => {
                let i = self.indices[level] as usize;
                (&leaf[i], i + 1 == leaf.len())
            }
            _ => unreachable!(),
        };

        // Advance the cursor
        if exhausted {
            self.path.pop();

            // Pop the path until we find the next sibling node
            while !self.path.is_empty() {
                let level = self.path.len() - 1;
                let i = self.indices[level] as usize + 1;
                let children = match self.path[level] {
                    NodeRef::Internal(inode) => &inode.children,
                    _ => unreachable!(),
                };
                if i >= children.len() {
                    self.path.pop();
                } else {
                    // Found a sibling node. Now, find the first element in it
                    self.indices[level] = i as _;
                    let mut cur = &children[i];
                    self.path.push(cur); // `[level + 1]`
                    loop {
                        let level = self.path.len() - 1;
                        self.indices[level] = 0;
                        match cur {
                            NodeRef::Internal(inode) => {
                                self.path.push(&inode.children[0]);
                                cur = &inode.children[0];
                            }
                            NodeRef::Leaf(_) => {
                                break;
                            }
                            NodeRef::Invalid => unreachable!(),
                        }
                    }
                    break;
                }
            }
        } else {
            self.indices[level] += 1;
        }

        Some((cursor, elem))
    }
}

impl<'a, T, O> FusedIterator for IterWithCursor<'a, T, O> {}
