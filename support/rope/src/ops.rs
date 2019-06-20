//! Rope operations (mostly private)
use arrayvec::ArrayVec;

use super::{Cursor, INode, NodeRef, Offset, Rope, ToOffset, ORDER};

impl<T, O> Rope<T, O>
where
    T: ToOffset<O>,
    O: Offset,
{
    /// Get a `Cursor` representing the last element.
    ///
    /// May panic or return an invalid value if the rope is empty.
    pub(crate) fn last_cursor(&self) -> Cursor {
        self.end_generic(false)
    }

    /// Get a `Cursor` representing the one-past-end position.
    pub(crate) fn end(&self) -> Cursor {
        self.end_generic(true)
    }

    /// The internal implementation of `end`
    fn end_generic(&self, past_end: bool) -> Cursor {
        let mut cursor = Cursor::default();

        let mut node = &self.root;
        loop {
            match node {
                NodeRef::Internal(inode) => {
                    cursor.indices.push((inode.children.len() - 1) as _);
                    node = inode.children.last().unwrap();
                }
                NodeRef::Leaf(leaf) => {
                    // Use the one-past-end index if `past_end` is `true`
                    cursor
                        .indices
                        .push((leaf.len() - (!past_end) as usize) as _);
                    break;
                }
                NodeRef::Invalid => unreachable!(),
            }
        }

        cursor
    }

    /// Get a `Cursor` representing the first element.
    pub(crate) fn begin(&self) -> Cursor {
        let mut cursor = Cursor::default();

        let mut node = &self.root;
        loop {
            cursor.indices.push(0);
            match node {
                NodeRef::Internal(inode) => {
                    node = inode.children.first().unwrap();
                }
                NodeRef::Leaf(_) => {
                    break;
                }
                NodeRef::Invalid => unreachable!(),
            }
        }

        cursor
    }

    /// Get the reference to the element specified by `at`.
    ///
    /// `at` must be a valid `Cursor` pointing at an element.
    pub(crate) fn get_at(&self, at: Cursor) -> &T {
        let mut it = at.indices.iter();
        let mut cur = &self.root;
        loop {
            // `Cursor::indices` contains a path to an element. The iterator
            // should return a value until we reach a leaf node.
            let i = *it.next().unwrap() as usize;
            match cur {
                NodeRef::Internal(inode) => {
                    cur = &inode.children[i];
                }
                NodeRef::Leaf(elements) => {
                    return &elements[i];
                }
                NodeRef::Invalid => unreachable!(),
            }
        }
    }

    /// Insert `x` before the element specified by `at`.
    pub(crate) fn insert(&mut self, x: T, at: Cursor) {
        let len = x.to_offset();

        if let Some((new_sibling, new_len)) = Self::insert_sub(&at.indices, &mut self.root, x, &len)
        {
            // Create a new root
            let old_root = std::mem::replace(&mut self.root, NodeRef::Invalid);

            let mut new_inode = Box::new(INode {
                children: ArrayVec::new(),
                offsets: ArrayVec::new(),
            });

            new_inode.children.push(old_root);
            new_inode.children.push(new_sibling);

            new_inode.offsets.push(new_len);

            self.root = NodeRef::Internal(new_inode);
        }

        self.len += len;
    }

    /// The internal method for `insert`.
    ///
    /// Returns `Some((new_node, len))` if it needs a new node that is a sibling
    /// of `node`. In this case, `len` indicates the new length of `node`
    /// `len` may or may not include `x_len` depending on which node `x` was
    /// inserted to.
    ///
    /// The algorithm is not recursive, but we need recursion to make borrowck
    /// happy.
    fn insert_sub(
        at: &[u8],
        node: &mut NodeRef<T, O>,
        x: T,
        x_len: &O,
    ) -> Option<(NodeRef<T, O>, O)> {
        if at.len() == 0 {
            unreachable!();
        }

        let i = at[0] as usize;

        if at.len() == 1 {
            // Leaf
            let elements = match node {
                NodeRef::Internal(_) => unreachable!(),
                NodeRef::Leaf(elements) => elements,
                NodeRef::Invalid => unreachable!(),
            };
            if elements.len() == elements.capacity() {
                // Full; split the leaf into two
                let mid = elements.capacity() / 2;

                let mut new_leaf = Box::new(ArrayVec::<[T; ORDER * 2]>::new());

                let mut second_half = elements.drain(mid..);

                // Prefer adding the new element to the newly created leaf
                // so that the number of bytes copied is minimized. Hence
                // the equality sign in this branch.
                if i >= mid {
                    // The new element belongs to the newly created leaf
                    new_leaf.extend((&mut second_half).take(i - mid));
                    new_leaf.push(x);
                    new_leaf.extend(second_half);
                } else {
                    // The new element belongs to the current leaf
                    new_leaf.extend(second_half);
                    elements.insert(i, x);
                }

                let first_half_len = elements
                    .iter()
                    .map(ToOffset::to_offset)
                    .fold(O::zero(), |x, y| x + y);

                Some((NodeRef::Leaf(new_leaf), first_half_len))
            } else {
                // The leaf is full, just insert it there
                elements.insert(i, x);
                None
            }
        } else {
            // Internal node
            let inode = match node {
                NodeRef::Internal(inode) => inode,
                NodeRef::Leaf(_) => unreachable!(),
                NodeRef::Invalid => unreachable!(),
            };

            for offset in inode.offsets[i..].iter_mut() {
                *offset += x_len.clone();
            }

            if let Some((new_sibling, new_len)) =
                Self::insert_sub(&at[1..], &mut inode.children[i], x, x_len)
            {
                // The child node has been split into two nodes.
                if inode.children.len() == inode.children.capacity() {
                    // Full; split the current internal node into two
                    let mid = inode.children.capacity() / 2;

                    let mut new_inode = Box::new(INode {
                        children: ArrayVec::new(),
                        offsets: ArrayVec::new(),
                    });

                    let mut second_half_children = inode.children.drain(mid..);
                    let mut second_half_offsets = inode.offsets.drain(mid - 1..);

                    let first_half_len = second_half_offsets.next().unwrap();

                    // Offsets are relative to the split point, so they should be
                    // adjusted when nodes are split
                    let mut second_half_offsets =
                        second_half_offsets.map(|i| i + -first_half_len.clone());

                    // This condition was chosen so that I only have to consider
                    // two cases, i.e., to exclude the case where
                    // `inode.children[i]` and `new_sibling` belong to different
                    // halves.
                    if i >= mid {
                        // `inode.children[i]` and `new_sibling` belongs to
                        // the second half
                        new_inode
                            .children
                            .extend((&mut second_half_children).take(i + 1 - mid));
                        new_inode.children.push(new_sibling);
                        new_inode.children.extend(second_half_children);

                        new_inode
                            .offsets
                            .extend((&mut second_half_offsets).take(i - mid));
                        if let Some(prev_len) = new_inode.offsets.last().cloned() {
                            new_inode.offsets.push(prev_len + new_len);
                        } else {
                            new_inode.offsets.push(new_len);
                        }
                        new_inode.offsets.extend(second_half_offsets);
                    } else {
                        // `inode.children[i]` and `new_sibling` belongs to
                        // the first half
                        new_inode.children.extend(second_half_children);
                        inode.children.insert(i + 1, new_sibling);

                        new_inode.offsets.extend(second_half_offsets);
                        if i == 0 {
                            inode.offsets.insert(i, new_len);
                        } else {
                            inode
                                .offsets
                                .insert(i, inode.offsets[i - 1].clone() + new_len);
                        }
                    }

                    debug_assert_eq!(inode.offsets.len(), inode.children.len() - 1);
                    debug_assert_eq!(new_inode.offsets.len(), new_inode.children.len() - 1);

                    Some((NodeRef::Internal(new_inode), first_half_len))
                } else {
                    // Not full
                    inode.children.insert(i + 1, new_sibling);
                    if i == 0 {
                        inode.offsets.insert(i, new_len);
                    } else {
                        inode
                            .offsets
                            .insert(i, inode.offsets[i - 1].clone() + new_len);
                    }
                    None
                }
            } else {
                None
            }
        }
    } // fn insert_sub

    /// Update the element specified by `at` using the function `f`.
    pub(crate) fn update_with(&mut self, at: Cursor, f: impl FnOnce(&mut T)) {
        let delta: O;

        let mut it = at.indices.iter();
        let mut cur = &mut self.root;
        loop {
            // `Cursor::indices` contains a path to an element. The iterator
            // should return a value until we reach a leaf node.
            let i = *it.next().unwrap() as usize;
            match cur {
                NodeRef::Internal(inode) => {
                    cur = &mut inode.children[i];
                }
                NodeRef::Leaf(elements) => {
                    let elem = &mut elements[i];
                    let old_len = (*elem).to_offset();

                    // Update the element
                    f(elem);

                    // Compute the length delta
                    delta = (*elem).to_offset() + -old_len;
                    break;
                }
                NodeRef::Invalid => unreachable!(),
            }
        }

        // Adjust offsets
        let mut it = at.indices.iter();
        let mut cur = &mut self.root;
        loop {
            // See the above comment about this `unwrap`.
            let i = *it.next().unwrap() as usize;
            match cur {
                NodeRef::Internal(inode) => {
                    for offset in inode.offsets[i..].iter_mut() {
                        *offset += delta.clone();
                    }
                    cur = &mut inode.children[i];
                }
                NodeRef::Leaf(_) => {
                    break;
                }
                NodeRef::Invalid => unreachable!(),
            }
        }
    }

    /// Remove the element specified by `at`.
    pub(crate) fn remove_at(&mut self, at: Cursor) -> T {
        let (elem, offset, underflow) = Self::remove_sub(&at.indices, &mut self.root);

        if underflow {
            // If an underflow flag is returned, we must check for the invariant
            // violation regarding the child count of the root `INode`.
            //
            // Note that `remove_sub` returns it whenever the child count goes
            // below `ORDER`, but for root inodes it's actually allowed to go
            // as low as `2`. Root leaves do not have a lower bound. So, this
            // might be a false alarm.
            Self::flatten_root_if_needed(&mut self.root);
        }

        self.len += -offset;
        elem
    }

    /// The internal method for `remove_at`. See the comment in `remove_at`.
    fn flatten_root_if_needed(node: &mut NodeRef<T, O>) {
        // Return early if it's a false alarm
        let child;
        match node {
            NodeRef::Internal(inode) => {
                if inode.children.len() >= 2 {
                    return;
                }

                // The invariant violation is a result of the removal of a
                // single element, so there should be at least one child to be
                // found here
                child = inode.children.pop().unwrap();
            }
            NodeRef::Leaf(_) => {
                return;
            }
            NodeRef::Invalid => unreachable!(),
        }

        // Move the only child to the top-level
        *node = child;
    }

    /// The internal method for `remove_at`.
    ///
    /// The algorithm is not recursive, but we need recursion to make borrowck
    /// happy.
    ///
    /// A return value `(elem, offset, underflow)` means the following:
    ///  - The removed element is `elem`.
    ///  - `offset` is the result of `elem.to_offset()`.
    ///  - `underflow` indicates if the new child count `node` is less than
    ///    `ORDER` or not.
    fn remove_sub(at: &[u8], node: &mut NodeRef<T, O>) -> (T, O, bool) {
        if at.len() == 0 {
            unreachable!();
        }

        let i = at[0] as usize;

        if at.len() == 1 {
            // Leaf
            let elements = match node {
                NodeRef::Internal(_) => unreachable!(),
                NodeRef::Leaf(elements) => elements,
                NodeRef::Invalid => unreachable!(),
            };

            // Remove an element
            let elem = elements.remove(i);
            let len = elem.to_offset();
            (elem, len, elements.len() < ORDER)
        } else {
            // Internal node
            let inode = match node {
                NodeRef::Internal(inode) => inode,
                NodeRef::Leaf(_) => unreachable!(),
                NodeRef::Invalid => unreachable!(),
            };

            let (elem, len, mut underflow) = Self::remove_sub(&at[1..], &mut inode.children[i]);

            for offset in inode.offsets[i..].iter_mut() {
                *offset += -len.clone();
            }

            if underflow {
                // `inode.children[i]` ran under the permitted minimum
                // child count, so rebalancing is required.

                let has_left = i > 0;
                let has_right = i + 1 < inode.children.len();

                // `INode` must have at least two children.
                debug_assert!(has_left || has_right);

                // Find the rebalancing strategy applicable for this situation.
                let mut use_left = has_left;
                let mut rotate = false;

                // Prefer rotation to merging.
                if has_right && inode.children[i + 1].len() > ORDER {
                    // Rotate left (move an element from the next silbling)
                    use_left = false;
                    rotate = true;
                } else if has_left && inode.children[i - 1].len() > ORDER {
                    // Rotate right (move an element from the previous silbling)
                    debug_assert!(use_left);
                    rotate = true;
                }

                // `children[i]` and one of its siblings
                // `i..=i+1` or `i-1..=i` depending on the value of `use_left`
                let k = i - use_left as usize;
                if let [left, right] = &mut inode.children[k..k + 2] {
                    let left_len = if k == 0 {
                        inode.offsets[0].clone()
                    } else {
                        inode.offsets[k].clone() + -inode.offsets[k - 1].clone()
                    };

                    if rotate {
                        // Rotate
                        let displacement;
                        if use_left {
                            displacement = -Self::rotate_right(left, right, left_len);
                        } else {
                            displacement = Self::rotate_left(left, right, left_len);
                        }

                        // The right edge of `children[k]` is translated by
                        // `displacement`. That of `children[k + 1]` is unchanged
                        // because we only moved an element between them.
                        inode.offsets[k] += displacement;

                        // We didn't change the child count of `inode`
                        underflow = false;
                    } else {
                        // Merge nodes

                        // Move all children from `right` to `left`
                        Self::rotate_left_full(left, right, left_len);

                        inode.children.remove(k + 1);
                        inode.offsets.remove(k);

                        // We changed the child count of `inode`, so `underflow`
                        // may be true
                        underflow = inode.children.len() < ORDER;
                    }
                } else {
                    unreachable!();
                } // if let [left, right] = ...
            } // if underflow

            (elem, len, underflow)
        }
    } // fn remove_sub

    /// Move the last child of `left` to the front of `right`.
    ///
    /// `left` and `right` must be of the same type, i.e. they must be one of
    /// `Leaf` and `Internal`.
    ///
    /// Return the length of the moved node.
    fn rotate_right(left: &mut NodeRef<T, O>, right: &mut NodeRef<T, O>, left_len: O) -> O {
        match (left, right) {
            (NodeRef::Leaf(elems1), NodeRef::Leaf(elems2)) => {
                let e = elems1.pop().unwrap();
                let len = e.to_offset();
                elems2.insert(0, e);
                len
            }
            (NodeRef::Internal(inode1), NodeRef::Internal(inode2)) => {
                let len = left_len + -inode1.offsets.last().unwrap().clone();

                let e = inode1.children.pop().unwrap();
                inode1.offsets.pop().unwrap();

                inode2.children.insert(0, e);
                for offset in inode2.offsets.iter_mut() {
                    *offset += len.clone();
                }
                inode2.offsets.insert(0, len.clone());

                len
            }
            _ => unreachable!(),
        }
    }

    /// Move the first child of `right` to the back of `left`.
    ///
    /// `left` and `right` must be of the same type, i.e. they must be one of
    /// `Leaf` and `Internal`.
    ///
    /// Return the length of the moved node.
    fn rotate_left(left: &mut NodeRef<T, O>, right: &mut NodeRef<T, O>, left_len: O) -> O {
        match (left, right) {
            (NodeRef::Leaf(elems1), NodeRef::Leaf(elems2)) => {
                let e = elems2.remove(0);
                let len = e.to_offset();
                elems1.push(e);
                len
            }
            (NodeRef::Internal(inode1), NodeRef::Internal(inode2)) => {
                let e = inode2.children.remove(0);
                let len = inode2.offsets.remove(0);
                for offset in inode2.offsets.iter_mut() {
                    *offset += -len.clone();
                }

                inode1.children.push(e);
                inode1.offsets.push(left_len);

                len
            }
            _ => unreachable!(),
        }
    }

    /// Move all children of `right` to the back of `left`.
    ///
    /// `left` and `right` must be of the same type, i.e. they must be one of
    /// `Leaf` and `Internal`.
    fn rotate_left_full(left: &mut NodeRef<T, O>, right: &mut NodeRef<T, O>, left_len: O) {
        match (left, right) {
            (NodeRef::Leaf(elems1), NodeRef::Leaf(elems2)) => {
                elems1.extend(elems2.drain(..));
            }
            (NodeRef::Internal(inode1), NodeRef::Internal(inode2)) => {
                inode1.children.extend(inode2.children.drain(..));

                inode1.offsets.push(left_len.clone());
                for offset in inode2.offsets.drain(..) {
                    inode1.offsets.push(offset + left_len.clone());
                }
            }
            _ => unreachable!(),
        }
    }
}
