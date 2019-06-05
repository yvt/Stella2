#![feature(const_vec_new)]
//! High-performance non-thread safe object pool (with an optional iteration
//! functionality).
//!
//! It also provides a type akin to pointers so you can realize linked list
//! data structures on it within the "safe" Rust. Memory safety is guaranteed by
//! runtime checks.
//!
//! Allocation Performance
//! ----------------------
//!
//! `Pool` outperformed Rust's default allocator (jemalloc) by at least twice
//! if each thread was given an exclusive access to an individual `Pool`.
//! It is expected that it will exhibit slightly better performance characteristics
//! on the real world use due to an improved spatial locality.
//!
//! It also comes with a sacrifice. It is impossible to return a free space to
//! the global heap without destroying entire the pool.
use std::{mem, ops};

pub mod intrusive_list;

/// High-performance non-thread safe object pool.
#[derive(Debug, Clone)]
pub struct Pool<T> {
    storage: Vec<Entry<T>>,
    first_free: Option<usize>,
}

/// High-performance non-thread safe object pool with an ability to iterate
/// through allocated objects.
#[derive(Debug, Clone)]
pub struct IterablePool<T> {
    storage: Vec<ItEntry<T>>,
    first_free: Option<usize>,
    first_used: Option<usize>,
}

/// A (potentially invalid) pointer to an object in `Pool`, but without
/// information about which specific `Pool` this is associated with.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct PoolPtr(pub usize);

#[derive(Debug, Clone)]
enum Entry<T> {
    Used(T),

    /// This entry is free. Points the next free entry.
    Free(Option<usize>),
}

#[derive(Debug, Clone)]
enum ItEntry<T> {
    /// This entry if occupied. Points the next and previous occupied entry
    /// (this forms a circular doubly-linked list).
    Used(T, (usize, usize)),

    /// This entry is free. Points the next free entry (forms a
    /// singly-linked list).
    Free(Option<usize>),
}

impl PoolPtr {
    /// Return an uninitialized pointer that has no guarantee regarding its
    /// usage with any `Pool`.
    ///
    /// This value can be used as a memory-efficient replacement for
    /// `Option<PoolPtr>` without a tag indicating whether it has a
    /// valid value or not.
    ///
    /// The returned pointer actually has a well-defined initialized value so
    /// using it will never result in an undefined behavior, hence this function
    /// is not marked with `unsafe`. It is just that it has no specific object
    /// or pool associated with it in a meaningful way.
    #[inline]
    pub fn uninitialized() -> Self {
        PoolPtr(0)
    }
}

impl<T> Entry<T> {
    fn as_ref(&self) -> Option<&T> {
        match self {
            &Entry::Used(ref value) => Some(value),
            &Entry::Free(_) => None,
        }
    }
    fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            &mut Entry::Used(ref mut value) => Some(value),
            &mut Entry::Free(_) => None,
        }
    }
    fn next_free_index(&self) -> Option<usize> {
        match self {
            &Entry::Used(_) => unreachable!(),
            &Entry::Free(i) => i,
        }
    }
}

impl<T> ItEntry<T> {
    fn as_ref(&self) -> Option<&T> {
        match self {
            &ItEntry::Used(ref value, _) => Some(value),
            &ItEntry::Free(_) => None,
        }
    }
    fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            &mut ItEntry::Used(ref mut value, _) => Some(value),
            &mut ItEntry::Free(_) => None,
        }
    }
    fn next_previous_used_index(&self) -> (usize, usize) {
        match self {
            &ItEntry::Used(_, (prev, next)) => (prev, next),
            &ItEntry::Free(_) => unreachable!(),
        }
    }
    fn next_previous_used_index_mut(&mut self) -> &mut (usize, usize) {
        match self {
            &mut ItEntry::Used(_, ref mut pn) => pn,
            &mut ItEntry::Free(_) => unreachable!(),
        }
    }
    fn next_free_index(&self) -> Option<usize> {
        match self {
            &ItEntry::Used(_, _) => unreachable!(),
            &ItEntry::Free(i) => i,
        }
    }
}

impl<T> Pool<T> {
    pub const fn new() -> Self {
        Self {
            storage: Vec::new(),
            first_free: None,
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        let mut pool = Self {
            storage: Vec::with_capacity(capacity),
            first_free: None,
        };
        if capacity > 0 {
            for i in 0..capacity - 1 {
                pool.storage.push(Entry::Free(Some(i + 1)));
            }
            pool.storage.push(Entry::Free(None));
            pool.first_free = Some(0);
        }
        pool
    }
    pub fn reserve(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        let existing_surplus = if self.first_free.is_some() {
            1 // at least one
        } else {
            0
        } + self.storage.capacity()
            - self.storage.len();
        if additional > existing_surplus {
            let needed_surplus =
                self.storage.capacity() - self.storage.len() + (additional - existing_surplus);
            self.storage.reserve(needed_surplus);
        }
    }
    pub fn allocate(&mut self, x: T) -> PoolPtr {
        match self.first_free {
            None => {
                self.storage.push(Entry::Used(x));
                PoolPtr(self.storage.len() - 1)
            }
            Some(i) => {
                let next_free = self.storage[i].next_free_index();
                self.first_free = next_free;
                self.storage[i] = Entry::Used(x);
                PoolPtr(i)
            }
        }
    }
    pub fn deallocate<S: Into<PoolPtr>>(&mut self, i: S) -> Option<T> {
        let i = i.into().0;
        let ref mut e = self.storage[i];
        match e {
            &mut Entry::Used(_) => {}
            &mut Entry::Free(_) => {
                return None;
            }
        }
        let x = match mem::replace(e, Entry::Free(self.first_free)) {
            Entry::Used(x) => x,
            Entry::Free(_) => unreachable!(),
        };
        self.first_free = Some(i);
        Some(x)
    }
    pub fn get(&self, fp: PoolPtr) -> Option<&T> {
        self.storage[fp.0].as_ref()
    }
    pub fn get_mut(&mut self, fp: PoolPtr) -> Option<&mut T> {
        self.storage[fp.0].as_mut()
    }
}

impl<T> IterablePool<T> {
    pub const fn new() -> Self {
        Self {
            storage: Vec::new(),
            first_free: None,
            first_used: None,
        }
    }
    pub fn with_capacity(capacity: usize) -> Self {
        let mut pool = Self {
            storage: Vec::with_capacity(capacity),
            first_free: None,
            first_used: None,
        };
        if capacity > 0 {
            for i in 0..capacity - 1 {
                pool.storage.push(ItEntry::Free(Some(i + 1)));
            }
            pool.storage.push(ItEntry::Free(None));
            pool.first_free = Some(0);
        }
        pool
    }
    pub fn reserve(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        let existing_surplus = if self.first_free.is_some() {
            1 // at least one
        } else {
            0
        } + self.storage.capacity()
            - self.storage.len();
        if additional > existing_surplus {
            let needed_surplus =
                self.storage.capacity() - self.storage.len() + (additional - existing_surplus);
            self.storage.reserve(needed_surplus);
        }
    }
    pub fn allocate(&mut self, x: T) -> PoolPtr {
        use std::mem::replace;

        if self.first_free.is_none() {
            self.storage.push(ItEntry::Free(None));
            self.first_free = Some(self.storage.len() - 1);
        }

        let i = self.first_free.unwrap();

        let next_prev = if let Some(first_used) = self.first_used {
            // Insert after the `self.first_used`
            let next = {
                let next_prev = self.storage[first_used].next_previous_used_index_mut();
                replace(&mut next_prev.0, i)
            };
            self.storage[next].next_previous_used_index_mut().1 = i;

            (next, first_used)
        } else {
            (i, i)
        };

        self.first_free = self.storage[i].next_free_index();
        self.storage[i] = ItEntry::Used(x, next_prev);
        self.first_used = Some(i);

        PoolPtr(i)
    }
    pub fn deallocate<S: Into<PoolPtr>>(&mut self, i: S) -> Option<T> {
        let i = i.into().0;
        let x = match mem::replace(&mut self.storage[i], ItEntry::Free(self.first_free)) {
            ItEntry::Used(x, (next, prev)) => {
                if next == i {
                    assert_eq!(self.first_used, Some(i));
                    assert_eq!(next, prev);
                    self.first_used = None;
                } else {
                    if self.first_used == Some(i) {
                        self.first_used = Some(next);
                    }
                    self.storage[next].next_previous_used_index_mut().1 = prev;
                    self.storage[prev].next_previous_used_index_mut().0 = next;
                }
                x
            }
            ItEntry::Free(_) => unreachable!(),
        };
        self.first_free = Some(i);
        Some(x)
    }
    pub fn get(&self, fp: PoolPtr) -> Option<&T> {
        self.storage[fp.0].as_ref()
    }
    pub fn get_mut(&mut self, fp: PoolPtr) -> Option<&mut T> {
        self.storage[fp.0].as_mut()
    }
    pub fn iter(&self) -> Iter<T> {
        Iter {
            pool: self,
            cur: self.first_used,
        }
    }
    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut {
            cur: self.first_used,
            pool: self,
        }
    }
}

impl<T> ops::Index<PoolPtr> for Pool<T> {
    type Output = T;

    fn index(&self, index: PoolPtr) -> &Self::Output {
        self.get(index).expect("dangling ptr")
    }
}

impl<T> ops::IndexMut<PoolPtr> for Pool<T> {
    fn index_mut(&mut self, index: PoolPtr) -> &mut Self::Output {
        self.get_mut(index).expect("dangling ptr")
    }
}

impl<T> ops::Index<PoolPtr> for IterablePool<T> {
    type Output = T;

    fn index(&self, index: PoolPtr) -> &Self::Output {
        self.get(index).expect("dangling ptr")
    }
}

impl<T> ops::IndexMut<PoolPtr> for IterablePool<T> {
    fn index_mut(&mut self, index: PoolPtr) -> &mut Self::Output {
        self.get_mut(index).expect("dangling ptr")
    }
}

/// An iterator over the elements of a `IterablePool`.
#[derive(Debug, Clone)]
pub struct Iter<'a, T: 'a> {
    pool: &'a IterablePool<T>,
    cur: Option<usize>,
}

impl<'a, T: 'a> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(cur) = self.cur {
            let ref entry = self.pool.storage[cur];
            self.cur = Some(entry.next_previous_used_index().0);
            if self.cur == self.pool.first_used {
                // Reached the end
                self.cur = None;
            }
            Some(entry.as_ref().unwrap())
        } else {
            None
        }
    }
}

/// A mutable iterator over the elements of a `IterablePool`.
#[derive(Debug)]
pub struct IterMut<'a, T: 'a> {
    pool: &'a mut IterablePool<T>,
    cur: Option<usize>,
}

impl<'a, T: 'a> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        use std::mem::transmute;
        if let Some(cur) = self.cur {
            // extend the lifetime of the mutable reference
            let entry: &mut ItEntry<_> = unsafe { transmute(&mut self.pool.storage[cur]) };
            self.cur = Some(entry.next_previous_used_index().0);
            if self.cur == self.pool.first_used {
                // Reached the end
                self.cur = None;
            }
            Some(entry.as_mut().unwrap())
        } else {
            None
        }
    }
}

#[test]
fn test() {
    let mut pool = Pool::new();
    let ptr1 = pool.allocate(1);
    let ptr2 = pool.allocate(2);
    assert_eq!(pool[ptr1], 1);
    assert_eq!(pool[ptr2], 2);
}

#[test]
#[should_panic]
fn dangling_ptr() {
    let mut pool = Pool::new();
    let ptr = pool.allocate(1);
    pool.deallocate(ptr);
    pool[ptr];
}
