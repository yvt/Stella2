use iterpool::{self, Pool};
use std::{
    ops,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolPtr {
    token: usize,
    inner: iterpool::PoolPtr,
}

/// Like `Pool<T>`, but `PoolPtr` is guaranteed to be unique.
#[derive(Debug)]
pub struct UniqPool<T> {
    pool: Pool<Entry<T>>,
}

#[derive(Debug)]
struct Entry<T> {
    token: usize,
    data: T,
}

#[allow(dead_code)]
impl<T> UniqPool<T> {
    pub const fn new() -> Self {
        Self { pool: Pool::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pool: Pool::with_capacity(capacity),
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        self.pool.reserve(additional);
    }

    pub fn allocate(&mut self, x: T) -> PoolPtr {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);

        if token == 0 {
            // Too many tokens were issued
            std::process::abort();
        }

        PoolPtr {
            token,
            inner: self.pool.allocate(Entry { token, data: x }),
        }
    }

    pub fn deallocate(&mut self, ptr: impl Into<PoolPtr>) -> Option<T> {
        let ptr = ptr.into();

        if let Some(iptr) = self.check_ptr(ptr) {
            self.pool.deallocate(iptr).map(|entry| entry.data)
        } else {
            None
        }
    }

    /// Validate `ptr` and get a pointer for the inner `pool`
    fn check_ptr(&self, ptr: PoolPtr) -> Option<iterpool::PoolPtr> {
        if let Some(entry) = &self.pool.get(ptr.inner) {
            if entry.token == ptr.token {
                Some(ptr.inner)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get(&self, ptr: PoolPtr) -> Option<&T> {
        self.check_ptr(ptr).map(|iptr| &self.pool[iptr].data)
    }

    pub fn get_mut(&mut self, ptr: PoolPtr) -> Option<&mut T> {
        if let Some(iptr) = self.check_ptr(ptr) {
            Some(&mut self.pool[iptr].data)
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ T> + '_ {
        self.pool.iter().map(|entry| &entry.data)
    }

    pub fn ptr_iter(&self) -> impl Iterator<Item = (PoolPtr, &'_ T)> + '_ {
        self.pool.ptr_iter().map(|(ptr, entry)| {
            (
                PoolPtr {
                    inner: ptr,
                    token: entry.token,
                },
                &entry.data,
            )
        })
    }
}

impl<T> ops::Index<PoolPtr> for UniqPool<T> {
    type Output = T;

    fn index(&self, index: PoolPtr) -> &Self::Output {
        self.get(index).expect("dangling ptr")
    }
}

impl<T> ops::IndexMut<PoolPtr> for UniqPool<T> {
    fn index_mut(&mut self, index: PoolPtr) -> &mut Self::Output {
        self.get_mut(index).expect("dangling ptr")
    }
}
