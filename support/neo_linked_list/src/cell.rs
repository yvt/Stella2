use std::{cell::UnsafeCell, pin::Pin};

use crate::linked_list::{LinkedList, Node};

/// `LinkedList` that can be mutated through a shared reference.
#[derive(Debug)]
pub struct LinkedListCell<T: ?Sized> {
    list: UnsafeCell<LinkedList<T>>,
}

unsafe impl<T: Send + ?Sized> Send for LinkedListCell<T> {}

impl<T: ?Sized> Default for LinkedListCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> LinkedListCell<T> {
    pub const fn new() -> Self {
        Self {
            list: UnsafeCell::new(LinkedList::new()),
        }
    }

    pub const fn from_inner(list: LinkedList<T>) -> Self {
        Self {
            list: UnsafeCell::new(list),
        }
    }

    pub fn into_inner(self) -> LinkedList<T> {
        self.list.into_inner()
    }

    pub fn replace(&self, src: LinkedList<T>) -> LinkedList<T> {
        std::mem::replace(unsafe { &mut *self.list.get() }, src)
    }

    pub fn take(&self) -> LinkedList<T> {
        self.replace(LinkedList::new())
    }

    pub fn get_mut(&mut self) -> &mut LinkedList<T> {
        unsafe { &mut *self.list.get() }
    }

    #[inline]
    pub fn clear(&self) {
        unsafe { &mut *self.list.get() }.clear();
    }

    pub fn len(&self) -> usize {
        unsafe { &*self.list.get() }.len()
    }

    pub fn is_empty(&self) -> bool {
        unsafe { &*self.list.get() }.is_empty()
    }

    /// Adds the given node to the front of the list.
    pub fn push_front_node(&self, node: Pin<Box<Node<T>>>) {
        unsafe { &mut *self.list.get() }.push_front_node(node);
    }

    /// Removes and returns the node at the front of the list.
    pub fn pop_front_node(&self) -> Option<Pin<Box<Node<T>>>> {
        unsafe { &mut *self.list.get() }.pop_front_node()
    }

    /// Adds the given node to the back of the list.
    pub fn push_back_node(&self, node: Pin<Box<Node<T>>>) {
        unsafe { &mut *self.list.get() }.push_back_node(node);
    }

    /// Removes and returns the node at the back of the list.
    pub fn pop_back_node(&self) -> Option<Pin<Box<Node<T>>>> {
        unsafe { &mut *self.list.get() }.pop_back_node()
    }
}

impl<T> LinkedListCell<T> {
    /// Adds an element first in the list.
    pub fn push_front(&self, elt: T) {
        unsafe { &mut *self.list.get() }.push_front(elt);
    }

    /// Removes the first element and returns it, or `None` if the list is
    /// empty.
    pub fn pop_front(&self) -> Option<T>
    where
        T: Unpin,
    {
        unsafe { &mut *self.list.get() }.pop_front()
    }

    /// Appends an element to the back of a list.
    pub fn push_back(&self, elt: T) {
        unsafe { &mut *self.list.get() }.push_back(elt);
    }

    /// Removes the last element from a list and returns it, or `None` if
    /// it is empty.
    pub fn pop_back(&self) -> Option<T>
    where
        T: Unpin,
    {
        unsafe { &mut *self.list.get() }.pop_back()
    }
}
