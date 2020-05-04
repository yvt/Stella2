//! A doubly-linked list with owned nodes.
//!
//! **This module is mostly based on [`linked_list.rs`] from the Rust standard
//! library.** There are the following differences:
//!
//!  - All features which are unstable at the point of writing were
//!    removed.
//!  - `LinkedList::split_off` was removed.
//!  - The element count accounting was removed. Counting the elements now takes
//!    a linear time.
//!  - The elements can now be unsized. `Node` has a room to store the `vtable`
//!    pointer.
//!  - The elements are pinned.
//!  - `Node` is exposed, making it possible to manipulate the elements which
//!    are pinned and/or unsized.
//!
//! [`linked_list.rs`]: https://github.com/rust-lang/rust/blob/5a1d028d4c8fc15473dc10473c38df162daa7b41/src/liballoc/collections/linked_list.rs
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::{FromIterator, FusedIterator};
use std::marker::{PhantomData, Unpin};
use std::mem;
use std::pin::Pin;
use std::ptr::NonNull;

#[cfg(test)]
mod tests;

/// A doubly-linked list with owned nodes.
///
/// The `LinkedList` allows pushing and popping elements at either end
/// in constant time.
///
/// NOTE: It is almost always better to use `Vec` or `VecDeque` because
/// array-based containers are generally faster,
/// more memory efficient, and make better use of CPU cache.
pub struct LinkedList<T: ?Sized> {
    head: Option<NonNull<Hdr>>,
    tail: Option<NonNull<Hdr>>,
    marker: PhantomData<Box<Node<T>>>,
}

#[repr(C)]
pub struct Node<T: ?Sized> {
    /// Must be the first field
    hdr: Hdr,
    pub element: T,
}

pub struct Hdr {
    next: Option<NonNull<Hdr>>,
    prev: Option<NonNull<Hdr>>,
    vtable: mem::MaybeUninit<*const ()>,
}

/// An iterator over the elements of a `LinkedList`.
///
/// This `struct` is created by the [`iter`] method on [`LinkedList`]. See its
/// documentation for more.
///
/// [`iter`]: struct.LinkedList.html#method.iter
/// [`LinkedList`]: struct.LinkedList.html
pub struct Iter<'a, T: 'a + ?Sized> {
    head: Option<NonNull<Hdr>>,
    tail: Option<NonNull<Hdr>>,
    marker: PhantomData<&'a Node<T>>,
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for Iter<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Iter").finish()
    }
}

// FIXME(#26925) Remove in favor of `#[derive(Clone)]`
impl<T: ?Sized> Clone for Iter<'_, T> {
    fn clone(&self) -> Self {
        Iter { ..*self }
    }
}

/// A mutable iterator over the elements of a `LinkedList`.
///
/// This `struct` is created by the [`iter_mut`] method on [`LinkedList`]. See its
/// documentation for more.
///
/// [`iter_mut`]: struct.LinkedList.html#method.iter_mut
/// [`LinkedList`]: struct.LinkedList.html
pub struct IterMut<'a, T: 'a + ?Sized> {
    // We do *not* exclusively own the entire list here, references to node's `element`
    // have been handed out by the iterator!  So be careful when using this; the methods
    // called must be aware that there can be aliasing pointers to `element`.
    list: &'a mut LinkedList<T>,
    head: Option<NonNull<Hdr>>,
    tail: Option<NonNull<Hdr>>,
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for IterMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("IterMut").field(&self.list).finish()
    }
}

/// An owning iterator over the elements of a `LinkedList`.
///
/// This `struct` is created by the [`into_iter`] method on [`LinkedList`][`LinkedList`]
/// (provided by the `IntoIterator` trait). See its documentation for more.
///
/// [`into_iter`]: struct.LinkedList.html#method.into_iter
/// [`LinkedList`]: struct.LinkedList.html
#[derive(Clone)]
pub struct IntoIter<T> {
    list: LinkedList<T>,
}

impl<T: fmt::Debug> fmt::Debug for IntoIter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("IntoIter").field(&self.list).finish()
    }
}

impl<T> Node<T> {
    pub fn new(element: T) -> Self {
        Node {
            hdr: Hdr::default(),
            element,
        }
    }

    /// A shortcut for `Box::pin(Node::new(x))`.
    pub fn pin(element: T) -> Pin<Box<Self>> {
        Box::pin(Node::new(element))
    }

    #[allow(clippy::boxed_local)]
    pub fn into_element(self: Box<Self>) -> T {
        self.element
    }
}

impl<T: ?Sized> Node<T> {
    pub fn element_pin(self: Pin<&Self>) -> Pin<&T> {
        unsafe { Pin::new_unchecked(&Pin::into_inner_unchecked(self).element) }
    }

    pub fn element_pin_mut(self: Pin<&mut Self>) -> Pin<&mut T> {
        unsafe { Pin::new_unchecked(&mut Pin::into_inner_unchecked(self).element) }
    }

    /// Update `self.hdr.vtable` if `&Self` is a fat pointer.
    #[inline]
    fn set_vtable(&mut self) {
        match mem::size_of::<&mut Self>() {
            x if x == mem::size_of::<usize>() => unsafe {
                // vtable is not needed
                let ptr: *const () = mem::transmute_copy(&self);

                // Ensure that `from_hdr` can recover `&Node<T>` from `&Hdr`
                // (1)
                assert_eq!(ptr, &self.hdr as *const _ as *const ());
            },
            x if x == mem::size_of::<usize>() * 2 => unsafe {
                let [ptr, vtable]: [*const (); 2] = mem::transmute_copy(&self);

                // Ensure that `from_hdr` can recover `&Node<T>` from `&Hdr`
                // (1)
                assert_eq!(ptr, &self.hdr as *const _ as *const ());
                // (2)
                self.hdr.vtable = mem::MaybeUninit::new(vtable);
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    fn box_into_hdr(mut self: Box<Self>) -> NonNull<Hdr> {
        self.set_vtable();

        NonNull::from(&mut Box::leak(self).hdr)
    }

    #[inline]
    unsafe fn from_hdr(hdr: NonNull<Hdr>) -> NonNull<Self> {
        let fatptr: [mem::MaybeUninit<*const ()>; 2] = [
            // (1)
            mem::MaybeUninit::new(hdr.as_ptr() as *const ()),
            // (2) If `&Self` is not a fat pointer, the following part is just
            // ignored
            hdr.as_ref().vtable,
        ];

        mem::transmute_copy(&fatptr)
    }

    #[inline]
    unsafe fn box_from_hdr(hdr: NonNull<Hdr>) -> Box<Self> {
        Box::from_raw(Self::from_hdr(hdr).as_ptr())
    }
}

impl Default for Hdr {
    fn default() -> Self {
        Hdr {
            next: None,
            prev: None,
            vtable: mem::MaybeUninit::uninit(),
        }
    }
}

// private methods
impl<T: ?Sized> LinkedList<T> {
    /// Adds the given node to the front of the list.
    pub fn push_front_node(&mut self, node: Pin<Box<Node<T>>>) {
        // This method takes care not to create mutable references to whole nodes,
        // to maintain validity of aliasing pointers into `element`.
        unsafe {
            let mut node = Pin::into_inner_unchecked(node);
            node.hdr.next = self.head;
            node.hdr.prev = None;
            let node = Some(node.box_into_hdr());

            match self.head {
                None => self.tail = node,
                // Not creating new mutable (unique!) references overlapping `element`.
                Some(head) => (*head.as_ptr()).prev = node,
            }

            self.head = node;
        }
    }

    /// Removes and returns the node at the front of the list.
    pub fn pop_front_node(&mut self) -> Option<Pin<Box<Node<T>>>> {
        // This method takes care not to create mutable references to whole nodes,
        // to maintain validity of aliasing pointers into `element`.
        self.head.map(|hdr| unsafe {
            let node = Node::box_from_hdr(hdr);
            self.head = node.hdr.next;

            match self.head {
                None => self.tail = None,
                // Not creating new mutable (unique!) references overlapping `element`.
                Some(head) => (*head.as_ptr()).prev = None,
            }

            Pin::new_unchecked(node)
        })
    }

    /// Adds the given node to the back of the list.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::{LinkedList, linked_list::Node};
    ///
    /// let mut d = LinkedList::<[u32]>::new();
    /// d.push_back_node(Node::pin([1]));
    /// d.push_back_node(Node::pin([1, 2, 3]));
    /// assert_eq!(&[1, 2, 3], d.back().unwrap());
    /// ```
    pub fn push_back_node(&mut self, node: Pin<Box<Node<T>>>) {
        // This method takes care not to create mutable references to whole nodes,
        // to maintain validity of aliasing pointers into `element`.
        unsafe {
            let mut node = Pin::into_inner_unchecked(node);
            node.hdr.next = None;
            node.hdr.prev = self.tail;
            let node = Some(node.box_into_hdr());

            match self.tail {
                None => self.head = node,
                // Not creating new mutable (unique!) references overlapping `element`.
                Some(tail) => (*tail.as_ptr()).next = node,
            }

            self.tail = node;
        }
    }

    /// Removes and returns the node at the back of the list.
    pub fn pop_back_node(&mut self) -> Option<Pin<Box<Node<T>>>> {
        // This method takes care not to create mutable references to whole nodes,
        // to maintain validity of aliasing pointers into `element`.
        self.tail.map(|hdr| unsafe {
            let node = Node::box_from_hdr(hdr);
            self.tail = node.hdr.prev;

            match self.tail {
                None => self.head = None,
                // Not creating new mutable (unique!) references overlapping `element`.
                Some(tail) => (*tail.as_ptr()).next = None,
            }

            Pin::new_unchecked(node)
        })
    }
}

impl<T> Default for LinkedList<T> {
    /// Creates an empty `LinkedList<T>`.
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> LinkedList<T> {
    /// Creates an empty `LinkedList`.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let list: LinkedList<u32> = LinkedList::new();
    /// ```
    #[inline]
    pub const fn new() -> Self {
        LinkedList {
            head: None,
            tail: None,
            marker: PhantomData,
        }
    }

    /// Moves all elements from `other` to the end of the list.
    ///
    /// This reuses all the nodes from `other` and moves them into `self`. After
    /// this operation, `other` becomes empty.
    ///
    /// This operation should compute in O(1) time and O(1) memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut list1 = LinkedList::new();
    /// list1.push_back('a');
    ///
    /// let mut list2 = LinkedList::new();
    /// list2.push_back('b');
    /// list2.push_back('c');
    ///
    /// list1.append(&mut list2);
    ///
    /// let mut iter = list1.iter();
    /// assert_eq!(iter.next(), Some(&'a'));
    /// assert_eq!(iter.next(), Some(&'b'));
    /// assert_eq!(iter.next(), Some(&'c'));
    /// assert!(iter.next().is_none());
    ///
    /// assert!(list2.is_empty());
    /// ```
    pub fn append(&mut self, other: &mut Self) {
        match self.tail {
            None => mem::swap(self, other),
            Some(mut tail) => {
                // `as_mut` is okay here because we have exclusive access to the entirety
                // of both lists.
                if let Some(mut other_head) = other.head.take() {
                    unsafe {
                        tail.as_mut().next = Some(other_head);
                        other_head.as_mut().prev = Some(tail);
                    }

                    self.tail = other.tail.take();
                }
            }
        }
    }

    /// Provides a forward iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut list: LinkedList<u32> = LinkedList::new();
    ///
    /// list.push_back(0);
    /// list.push_back(1);
    /// list.push_back(2);
    ///
    /// let mut iter = list.iter();
    /// assert_eq!(iter.next(), Some(&0));
    /// assert_eq!(iter.next(), Some(&1));
    /// assert_eq!(iter.next(), Some(&2));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            head: self.head,
            tail: self.tail,
            marker: PhantomData,
        }
    }

    /// Provides a forward iterator with mutable references.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut list: LinkedList<u32> = LinkedList::new();
    ///
    /// list.push_back(0);
    /// list.push_back(1);
    /// list.push_back(2);
    ///
    /// for element in list.iter_mut() {
    ///     *element += 10;
    /// }
    ///
    /// let mut iter = list.iter();
    /// assert_eq!(iter.next(), Some(&10));
    /// assert_eq!(iter.next(), Some(&11));
    /// assert_eq!(iter.next(), Some(&12));
    /// assert_eq!(iter.next(), None);
    /// ```
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            head: self.head,
            tail: self.tail,
            list: self,
        }
    }

    /// Returns `true` if the `LinkedList` is empty.
    ///
    /// This operation should compute in O(1) time.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    /// assert!(dl.is_empty());
    ///
    /// dl.push_front("foo");
    /// assert!(!dl.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Returns the length of the `LinkedList`.
    ///
    /// **This operation computes in O(N) time** unlike the original
    /// implementation of `LinkedList`.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    ///
    /// dl.push_front(2);
    /// assert_eq!(dl.len(), 1);
    ///
    /// dl.push_front(1);
    /// assert_eq!(dl.len(), 2);
    ///
    /// dl.push_back(3);
    /// assert_eq!(dl.len(), 3);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Removes all elements from the `LinkedList`.
    ///
    /// This operation should compute in O(n) time.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    ///
    /// dl.push_front(2);
    /// dl.push_front(1);
    /// assert_eq!(dl.len(), 2);
    /// assert_eq!(dl.front(), Some(&1));
    ///
    /// dl.clear();
    /// assert_eq!(dl.len(), 0);
    /// assert_eq!(dl.front(), None);
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Returns `true` if the `LinkedList` contains an element equal to the
    /// given value.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut list: LinkedList<u32> = LinkedList::new();
    ///
    /// list.push_back(0);
    /// list.push_back(1);
    /// list.push_back(2);
    ///
    /// assert_eq!(list.contains(&0), true);
    /// assert_eq!(list.contains(&10), false);
    /// ```
    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq<T>,
    {
        self.iter().any(|e| e == x)
    }

    /// Provides a reference to the front element, or `None` if the list is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    /// assert_eq!(dl.front(), None);
    ///
    /// dl.push_front(1);
    /// assert_eq!(dl.front(), Some(&1));
    /// ```
    #[inline]
    pub fn front(&self) -> Option<&T>
    where
        T: Unpin,
    {
        self.front_pin().map(Pin::into_inner)
    }

    #[inline]
    pub fn front_pin(&self) -> Option<Pin<&T>> {
        self.front_node().map(Node::element_pin)
    }

    #[inline]
    pub fn front_node(&self) -> Option<Pin<&Node<T>>> {
        unsafe {
            self.head
                .as_ref()
                .map(|hdr| Pin::new_unchecked(&*Node::from_hdr(*hdr).as_ptr()))
        }
    }

    /// Provides a mutable reference to the front element, or `None` if the list
    /// is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    /// assert_eq!(dl.front(), None);
    ///
    /// dl.push_front(1);
    /// assert_eq!(dl.front(), Some(&1));
    ///
    /// match dl.front_mut() {
    ///     None => {},
    ///     Some(x) => *x = 5,
    /// }
    /// assert_eq!(dl.front(), Some(&5));
    /// ```
    #[inline]
    pub fn front_mut(&mut self) -> Option<&mut T>
    where
        T: Unpin,
    {
        self.front_pin_mut().map(Pin::into_inner)
    }

    #[inline]
    pub fn front_pin_mut(&mut self) -> Option<Pin<&mut T>> {
        unsafe { self.front_node_mut() }.map(Node::element_pin_mut)
    }

    /// Provides a mutable reference to the front element's node, or `None` if
    /// the list is empty.
    ///
    /// # Safety
    ///
    // This method is unsafe because it allows replacing the returned
    // node in-place, corrupting the structure.
    #[inline]
    pub unsafe fn front_node_mut(&mut self) -> Option<Pin<&mut Node<T>>> {
        self.head
            .map(|hdr| Pin::new_unchecked(&mut *Node::from_hdr(hdr).as_ptr()))
    }

    /// Provides a reference to the back element, or `None` if the list is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    /// assert_eq!(dl.back(), None);
    ///
    /// dl.push_back(1);
    /// assert_eq!(dl.back(), Some(&1));
    /// ```
    #[inline]
    pub fn back(&self) -> Option<&T>
    where
        T: Unpin,
    {
        self.back_pin().map(Pin::into_inner)
    }

    #[inline]
    pub fn back_pin(&self) -> Option<Pin<&T>> {
        self.back_node().map(Node::element_pin)
    }

    #[inline]
    pub fn back_node(&self) -> Option<Pin<&Node<T>>> {
        self.tail
            .map(|hdr| unsafe { Pin::new_unchecked(&*Node::from_hdr(hdr).as_ptr()) })
    }

    /// Provides a mutable reference to the back element, or `None` if the list
    /// is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    /// assert_eq!(dl.back(), None);
    ///
    /// dl.push_back(1);
    /// assert_eq!(dl.back(), Some(&1));
    ///
    /// match dl.back_mut() {
    ///     None => {},
    ///     Some(x) => *x = 5,
    /// }
    /// assert_eq!(dl.back(), Some(&5));
    /// ```
    #[inline]
    pub fn back_mut(&mut self) -> Option<&mut T>
    where
        T: Unpin,
    {
        self.back_pin_mut().map(Pin::into_inner)
    }

    #[inline]
    pub fn back_pin_mut(&mut self) -> Option<Pin<&mut T>> {
        unsafe { self.back_node_mut() }.map(Node::element_pin_mut)
    }

    /// Provides a mutable reference to the back element's node, or `None` if
    /// the list is empty.
    ///
    /// # Safety
    ///
    // This method is unsafe because it allows replacing the returned
    // node in-place, corrupting the structure.
    #[inline]
    pub unsafe fn back_node_mut(&mut self) -> Option<Pin<&mut Node<T>>> {
        self.tail
            .map(|hdr| Pin::new_unchecked(&mut *Node::from_hdr(hdr).as_ptr()))
    }
}

impl<T> LinkedList<T> {
    /// Adds an element first in the list.
    ///
    /// This operation should compute in O(1) time.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut dl = LinkedList::new();
    ///
    /// dl.push_front(2);
    /// assert_eq!(dl.front().unwrap(), &2);
    ///
    /// dl.push_front(1);
    /// assert_eq!(dl.front().unwrap(), &1);
    /// ```
    pub fn push_front(&mut self, elt: T) {
        self.push_front_node(Box::pin(Node::new(elt)));
    }

    /// Removes the first element and returns it, or `None` if the list is
    /// empty.
    ///
    /// This operation should compute in O(1) time.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut d = LinkedList::new();
    /// assert_eq!(d.pop_front(), None);
    ///
    /// d.push_front(1);
    /// d.push_front(3);
    /// assert_eq!(d.pop_front(), Some(3));
    /// assert_eq!(d.pop_front(), Some(1));
    /// assert_eq!(d.pop_front(), None);
    /// ```
    pub fn pop_front(&mut self) -> Option<T>
    where
        T: Unpin,
    {
        self.pop_front_node()
            .map(Pin::into_inner)
            .map(Node::into_element)
    }

    /// Appends an element to the back of a list.
    ///
    /// This operation should compute in O(1) time.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut d = LinkedList::new();
    /// d.push_back(1);
    /// d.push_back(3);
    /// assert_eq!(3, *d.back().unwrap());
    /// ```
    pub fn push_back(&mut self, elt: T) {
        self.push_back_node(Box::pin(Node::new(elt)));
    }

    /// Removes the last element from a list and returns it, or `None` if
    /// it is empty.
    ///
    /// This operation should compute in O(1) time.
    ///
    /// # Examples
    ///
    /// ```
    /// use neo_linked_list::LinkedList;
    ///
    /// let mut d = LinkedList::new();
    /// assert_eq!(d.pop_back(), None);
    /// d.push_back(1);
    /// d.push_back(3);
    /// assert_eq!(d.pop_back(), Some(3));
    /// ```
    pub fn pop_back(&mut self) -> Option<T>
    where
        T: Unpin,
    {
        self.pop_back_node()
            .map(Pin::into_inner)
            .map(Node::into_element)
    }
}

impl<T: ?Sized> Drop for LinkedList<T> {
    fn drop(&mut self) {
        while let Some(_) = self.pop_front_node() {}
    }
}

impl<'a, T: ?Sized> Iterator for Iter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<&'a T> {
        self.head.map(|hdr| unsafe {
            // Need an unbound lifetime to get 'a
            let node = &*Node::from_hdr(hdr).as_ptr();
            self.head = node.hdr.next;
            &node.element
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.head.is_none() {
            (0, Some(0))
        } else {
            (1, None)
        }
    }

    #[inline]
    fn last(mut self) -> Option<&'a T> {
        self.next_back()
    }
}

impl<'a, T: ?Sized> DoubleEndedIterator for Iter<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a T> {
        self.tail.map(|hdr| unsafe {
            // Need an unbound lifetime to get 'a
            let node = &*Node::from_hdr(hdr).as_ptr();
            self.tail = node.hdr.prev;
            &node.element
        })
    }
}

impl<T: ?Sized> ExactSizeIterator for Iter<'_, T> {}

impl<T: ?Sized> FusedIterator for Iter<'_, T> {}

impl<'a, T: ?Sized> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<&'a mut T> {
        self.head.map(|hdr| unsafe {
            // Need an unbound lifetime to get 'a
            let node = &mut *Node::from_hdr(hdr).as_ptr();
            self.head = node.hdr.next;
            &mut node.element
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.head.is_none() {
            (0, Some(0))
        } else {
            (1, None)
        }
    }

    #[inline]
    fn last(mut self) -> Option<&'a mut T> {
        self.next_back()
    }
}

impl<'a, T: ?Sized> DoubleEndedIterator for IterMut<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<&'a mut T> {
        self.tail.map(|hdr| unsafe {
            // Need an unbound lifetime to get 'a
            let node = &mut *Node::from_hdr(hdr).as_ptr();
            self.tail = node.hdr.prev;
            &mut node.element
        })
    }
}

impl<T: ?Sized> ExactSizeIterator for IterMut<'_, T> {}

impl<T: ?Sized> FusedIterator for IterMut<'_, T> {}

impl<T: Unpin> Iterator for IntoIter<T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        self.list.pop_front()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.list.is_empty() {
            (0, Some(0))
        } else {
            (1, None)
        }
    }
}

impl<T: Unpin> DoubleEndedIterator for IntoIter<T> {
    #[inline]
    fn next_back(&mut self) -> Option<T> {
        self.list.pop_back()
    }
}

impl<T: Unpin> ExactSizeIterator for IntoIter<T> {}

impl<T: Unpin> FusedIterator for IntoIter<T> {}

impl<'a, T: 'a + Copy> Extend<&'a T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        for x in iter {
            self.push_back(*x);
        }
    }
}

impl<T> Extend<T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for x in iter {
            self.push_back(x);
        }
    }
}

impl<T> FromIterator<T> for LinkedList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut list = Self::new();
        list.extend(iter);
        list
    }
}

impl<T: Unpin> IntoIterator for LinkedList<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    /// Consumes the list into an iterator yielding elements by value.
    #[inline]
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { list: self }
    }
}

impl<'a, T: ?Sized> IntoIterator for &'a LinkedList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<'a, T: ?Sized> IntoIterator for &'a mut LinkedList<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}

impl<T: PartialEq + ?Sized> PartialEq for LinkedList<T> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other)
    }

    #[allow(clippy::partialeq_ne_impl)]
    fn ne(&self, other: &Self) -> bool {
        self.iter().ne(other)
    }
}

impl<T: Eq + ?Sized> Eq for LinkedList<T> {}

impl<T: PartialOrd + ?Sized> PartialOrd for LinkedList<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.iter().partial_cmp(other)
    }
}

impl<T: Ord + ?Sized> Ord for LinkedList<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other)
    }
}

impl<T: Clone> Clone for LinkedList<T> {
    fn clone(&self) -> Self {
        self.iter().cloned().collect()
    }
}

impl<T: fmt::Debug + ?Sized> fmt::Debug for LinkedList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self).finish()
    }
}

impl<T: Hash + ?Sized> Hash for LinkedList<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut len = 0;
        for elt in self {
            elt.hash(state);
            len += 1;
        }
        len.hash(state);
    }
}

// Ensure that `LinkedList` and its read-only iterators are covariant in their type parameters.
#[allow(dead_code)]
fn assert_covariance() {
    fn a<'a>(x: LinkedList<&'static str>) -> LinkedList<&'a str> {
        x
    }
    fn b<'i, 'a>(x: Iter<'i, &'static str>) -> Iter<'i, &'a str> {
        x
    }
    fn c<'a>(x: IntoIter<&'static str>) -> IntoIter<&'a str> {
        x
    }
}

unsafe impl<T: Send + ?Sized> Send for LinkedList<T> {}

unsafe impl<T: Sync + ?Sized> Sync for LinkedList<T> {}

unsafe impl<T: Send + ?Sized> Send for Node<T> {}

unsafe impl<T: Sync + ?Sized> Sync for Node<T> {}

unsafe impl<T: Sync + ?Sized> Send for Iter<'_, T> {}

unsafe impl<T: Sync + ?Sized> Sync for Iter<'_, T> {}

unsafe impl<T: Send + ?Sized> Send for IterMut<'_, T> {}

unsafe impl<T: Sync + ?Sized> Sync for IterMut<'_, T> {}
