//! Provides a type representing a list of subscribers. When adding an
//! element, the caller gets a ticket for deleting (i.e., unsubscribing) that
//! element.
use iterpool::{self, IterablePool, PoolPtr};
use std::{
    cell::{Ref, RefCell, RefMut},
    fmt,
    mem::transmute,
    rc::{Rc, Weak},
};

/// A type representing a list of subscribers.
#[derive(Debug)]
pub struct SubscriberList<T> {
    pool: Rc<RefCell<IterablePool<T>>>,
}

/// An element (subscriber) in [`SubscriberList`].
#[derive(Debug)]
pub struct Subscription<T> {
    pool: Weak<RefCell<IterablePool<T>>>,
    ptr: PoolPtr,
}

impl<T> Default for SubscriberList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SubscriberList<T> {
    pub fn new() -> Self {
        Self {
            pool: Rc::new(RefCell::new(IterablePool::new())),
        }
    }

    /// Insert an element to a subscriber list.
    ///
    /// Returns a token that can be used to remove the inserted element.
    pub fn insert(&mut self, x: T) -> Subscription<T> {
        let mut pool = self.pool.borrow_mut();
        let ptr = pool.allocate(x);
        Subscription {
            pool: Rc::downgrade(&self.pool),
            ptr,
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        // This `borrow()` always succeeds because of the following reasons:
        //  1. The calls to `borrow` and `borrow_mut` in this `impl` follows the
        //     receiver mutability of the calling methods
        //  2. These methods are never called when `unsubscribe` has a mutable
        //     borrow.
        let borrow = self.pool.borrow();
        let inner = unsafe { transmute(borrow.iter()) };
        Iter { borrow, inner }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let mut borrow = self.pool.borrow_mut();
        let inner = unsafe { transmute(borrow.iter_mut()) };
        IterMut { borrow, inner }
    }
}

impl<T> Subscription<T> {
    /// Remove the element that `self` represents.
    ///
    /// Returns `Some(x)` if the element `x` was removed; `None` if the list
    /// has already been dropped.
    pub fn unsubscribe(self) -> Result<Option<T>, IterationActive> {
        if let Some(pool) = self.pool.upgrade() {
            let mut pool = pool.try_borrow_mut().map_err(|_| IterationActive)?;
            Ok(Some(pool.deallocate(self.ptr).unwrap()))
        } else {
            Ok(None)
        }
    }

    pub fn untype(self) -> UntypedSubscription
    where
        T: 'static,
    {
        UntypedSubscription {
            pool: self.pool,
            ptr: self.ptr,
        }
    }
}

#[derive(Debug)]
pub struct Iter<'a, T> {
    borrow: Ref<'a, IterablePool<T>>,
    inner: iterpool::Iter<'a, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

#[derive(Debug)]
pub struct IterMut<'a, T> {
    borrow: RefMut<'a, IterablePool<T>>,
    inner: iterpool::IterMut<'a, T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Type-erased [`Subscription`].
pub struct UntypedSubscription {
    pool: Weak<RefCell<dyn ErasedPool>>,
    ptr: PoolPtr,
}

trait ErasedPool {
    fn deallocate(&mut self, ptr: PoolPtr);
}

impl<T> ErasedPool for IterablePool<T> {
    fn deallocate(&mut self, ptr: PoolPtr) {
        self.deallocate(ptr);
    }
}

enum Never {}

impl ErasedPool for Never {
    fn deallocate(&mut self, _: PoolPtr) {
        match *self {}
    }
}

impl fmt::Debug for UntypedSubscription {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("UntypedSubscription")
            .field("ptr", &self.ptr)
            .finish()
    }
}

impl Default for UntypedSubscription {
    fn default() -> Self {
        Self::new()
    }
}

impl UntypedSubscription {
    /// Construct an `UntypedSubscription` that refers to no backing object.
    pub fn new() -> Self {
        Self {
            pool: Weak::<RefCell<Never>>::new(),
            ptr: PoolPtr::uninitialized(),
        }
    }

    /// Remove the element that `self` represents.
    pub fn unsubscribe(self) -> Result<Option<()>, IterationActive> {
        if let Some(pool) = self.pool.upgrade() {
            let mut pool = pool.try_borrow_mut().map_err(|_| IterationActive)?;
            Ok(Some(pool.deallocate(self.ptr)))
        } else {
            Ok(None)
        }
    }
}

/// An error type returned when a subscription could not be removed because
/// there is an active iterator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IterationActive;

impl fmt::Display for IterationActive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "there is an ongoing iteration")
    }
}

impl std::error::Error for IterationActive {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_subscription() {
        let mut list = SubscriberList::new();
        let ss = list.insert(1);
        assert_eq!(list.iter().cloned().collect::<Vec<_>>(), vec![1]);
        assert_eq!(list.iter_mut().map(|x| *x).collect::<Vec<_>>(), vec![1]);
        assert_eq!(ss.unsubscribe(), Ok(Some(1)));
        assert_eq!(list.iter().cloned().collect::<Vec<_>>(), vec![]);
    }

    #[test]
    fn iteration_active() {
        let mut list = SubscriberList::new();
        let ss = list.insert(1);
        let _it = list.iter();
        assert_eq!(ss.unsubscribe(), Err(IterationActive));
    }

    #[test]
    fn list_dropped() {
        let mut list = SubscriberList::new();
        let ss = list.insert(1);
        drop(list);
        assert_eq!(ss.unsubscribe(), Ok(None));
    }

    #[test]
    fn untyped_remove_subscription() {
        let mut list = SubscriberList::new();
        let ss = list.insert(1).untype();
        assert_eq!(list.iter().cloned().collect::<Vec<_>>(), vec![1]);
        assert_eq!(list.iter_mut().map(|x| *x).collect::<Vec<_>>(), vec![1]);
        assert_eq!(ss.unsubscribe(), Ok(Some(())));
        assert_eq!(list.iter().cloned().collect::<Vec<_>>(), vec![]);
    }

    #[test]
    fn untyped_iteration_active() {
        let mut list = SubscriberList::new();
        let ss = list.insert(1).untype();
        let _it = list.iter();
        assert_eq!(ss.unsubscribe(), Err(IterationActive));
    }

    #[test]
    fn untyped_list_dropped() {
        let mut list = SubscriberList::new();
        let ss = list.insert(1).untype();
        drop(list);
        assert_eq!(ss.unsubscribe(), Ok(None));
    }
}
