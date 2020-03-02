//! Provides an implementation of an object bag that can be safely indexed by
//! `PoolPtr` whether the referenced objects are alive or not.
//!
//! `LeakyPool` allocates memory regions by leaky allocation
//! (`Box::leak(Box::new(...))`). This way, `PoolPtr` can directly point to the
//! underlying object and doesn't require reference counting. Vacant regions are
//! collected by `LeakyPool` for later reuse, thus bounding the memory usage,
//! assuming only a predetermined number of `LeakyPool` are created and used
//! throughout the program's lifetime. `LeakyPool` leaks memory only when
//! `LeakyPool` is dropped.
use std::{fmt, hint::unreachable_unchecked, marker::PhantomData, ops};
use tokenlock::TokenLock;
use try_match::try_match;

mod token;
pub use self::token::*;

/// An object bag that can be safely indexed by [`PoolPtr`] whether
/// the referenced objects are alive or not.
pub struct LeakyPool<Element: 'static, TokenStoreTy: TokenStore = LazyToken<LeakyToken>> {
    token_store: TokenStoreTy,
    _elements_phantom: PhantomData<Element>,
    /// A linked list of vacant entries. All `TokenLock`s in the list must be
    /// associated with `self.token_store`.
    first_free: Option<PoolPtr<Element, TokenStoreTy::TokenId>>,
}

impl<Element: 'static, TokenStoreTy: TokenStore + fmt::Debug> fmt::Debug
    for LeakyPool<Element, TokenStoreTy>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("LeakyPool")
            .field("token_store", &self.token_store)
            .finish()
    }
}

pub struct PoolPtr<Element: 'static, TokenId: 'static = LeakyTokenId> {
    entry: &'static Entry<Element, TokenId>,
}

impl<Element: 'static, TokenId: 'static + fmt::Debug> fmt::Debug for PoolPtr<Element, TokenId> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("PoolPtr").field(&self.entry.lock).finish()
    }
}

impl<Element: 'static, TokenId: 'static> Clone for PoolPtr<Element, TokenId> {
    fn clone(&self) -> Self {
        Self { entry: self.entry }
    }
}

impl<Element: 'static, TokenId: 'static> Copy for PoolPtr<Element, TokenId> {}

impl<Element: 'static, TokenId: 'static> PartialEq for PoolPtr<Element, TokenId> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.entry, other.entry)
    }
}

impl<Element: 'static, TokenId: 'static> Eq for PoolPtr<Element, TokenId> {}

struct Entry<Element: 'static, TokenId: 'static> {
    lock: TokenLock<EntryState<Element, TokenId>, TokenId>,
}

enum EntryState<Element: 'static, TokenId: 'static> {
    /// The entry is occupied.
    Occupied(Element),
    /// The entry is vacant. Points the next vacant entry, forming a linked list
    /// headed by `LeakyPool::first_free`.
    Vacant(Option<PoolPtr<Element, TokenId>>),
}

impl<Element, Token> LeakyPool<Element, LazyToken<Token>>
where
    LazyToken<Token>: TokenStore,
{
    /// Construct a `LeakyPool<Element, LazyToken<_>>`.
    ///
    /// Ideally this should be generalized over all `TokenStoreTy: Default`, but
    /// due to the language restrictions, `Default` can't incorporate
    /// `const`-ness at the moment.
    pub const fn new() -> Self {
        // TODO: Delegate to `with_token_store` when it's `const fn`
        Self {
            token_store: LazyToken::new(),
            _elements_phantom: PhantomData,
            first_free: None,
        }
    }
}

impl<Element> LeakyPool<Element, UncheckedToken> {
    /// Construct a `LeakyPool<Element, UncheckedToken>`.
    ///
    /// This will be superseded by `with_token_store` when `const fn` with
    /// trait bounds on type parameters is stabilized.
    pub const unsafe fn new_unchecked() -> Self {
        // TODO: Delegate to `with_token_store` when it's `const fn`
        Self {
            token_store: UncheckedToken::new(),
            _elements_phantom: PhantomData,
            first_free: None,
        }
    }
}

impl<Element, TokenStoreTy> LeakyPool<Element, TokenStoreTy>
where
    TokenStoreTy: TokenStore,
{
    /// Construct a `LeakyPool`.
    pub fn with_token_store(token_store: TokenStoreTy) -> Self {
        // TODO: Make this `const fn` when trait bounds for `const fn` parameters
        //       are stable (`#![feature(const_fn)]`)
        Self {
            token_store,
            _elements_phantom: PhantomData,
            first_free: None,
        }
    }

    pub fn allocate(&mut self, x: Element) -> PoolPtr<Element, TokenStoreTy::TokenId> {
        // Get `PoolPtr` for the new element.
        let ptr = self.first_free.take().unwrap_or_else(|| {
            // `first_free` is empty. Construct a new vacant entry
            let token_id = self.token_store.id();

            let entry = Box::leak(Box::new(Entry {
                lock: TokenLock::new(token_id, EntryState::Vacant(None)),
            }));

            PoolPtr { entry }
        });

        // Access the entry referred by `ptr`.
        // All entires in the linked list `self.first_free` are associated with
        // `self.token_store`, so this is okay
        let entry_state = ptr
            .entry
            .lock
            .write(self.token_store.token_mut())
            .unwrap_or_else(|| unsafe { unreachable_unchecked() });

        // Assign the new element.
        let old_state = std::mem::replace(entry_state, EntryState::Occupied(x));

        // Update `first_free` to point to the next vacant entry (if there's one).
        // This `unsafe` is safe because all entires in the linked list
        // `self.first_free` are supposed to be vacant.
        let next_ptr = try_match!(EntryState::Vacant(next_ptr) = old_state)
            .unwrap_or_else(|_| unsafe { unreachable_unchecked() });
        self.first_free = next_ptr;

        ptr
    }

    pub fn deallocate(&mut self, ptr: PoolPtr<Element, TokenStoreTy::TokenId>) -> Option<Element> {
        // Get a mutable reference to the entry's `EntryState`. Return `None`
        // if `ptr` belongs to a different `LeakyPool`.
        let entry_state = ptr.entry.lock.write(self.token_store.token_mut())?;

        // Return `None` if `ptr` refers to a vacant entry.
        if matches!(&*entry_state, EntryState::Vacant(_)) {
            return None;
        }

        let new_state = EntryState::Vacant(self.first_free);

        // Take the contained element and store `new_state` pointing to the
        // first vacant entry. `unwrap` here is infallible since we've already
        // confirmed that it's `EntryState::Occupied` just above here.
        let taken_state = std::mem::replace(entry_state, new_state);
        let x = try_match!(EntryState::Occupied(element) = taken_state)
            .ok()
            .unwrap_or_else(|| std::process::abort());

        // Update the free list.
        self.first_free = Some(ptr);

        Some(x)
    }

    pub fn get(&self, ptr: PoolPtr<Element, TokenStoreTy::TokenId>) -> Option<&Element> {
        (ptr.entry.lock)
            .read(self.token_store.token_ref())
            .and_then(|entry_state| try_match!(EntryState::Occupied(element) = entry_state).ok())
    }

    pub fn get_mut(
        &mut self,
        ptr: PoolPtr<Element, TokenStoreTy::TokenId>,
    ) -> Option<&mut Element> {
        (ptr.entry.lock)
            .write(self.token_store.token_mut())
            .and_then(|entry_state| try_match!(EntryState::Occupied(element) = entry_state).ok())
    }
}

impl<Element, TokenStoreTy> ops::Index<PoolPtr<Element, TokenStoreTy::TokenId>>
    for LeakyPool<Element, TokenStoreTy>
where
    TokenStoreTy: TokenStore,
{
    type Output = Element;

    fn index(&self, index: PoolPtr<Element, TokenStoreTy::TokenId>) -> &Self::Output {
        self.get(index).expect("dangling ptr")
    }
}

impl<Element, TokenStoreTy> ops::IndexMut<PoolPtr<Element, TokenStoreTy::TokenId>>
    for LeakyPool<Element, TokenStoreTy>
where
    TokenStoreTy: TokenStore,
{
    fn index_mut(&mut self, index: PoolPtr<Element, TokenStoreTy::TokenId>) -> &mut Self::Output {
        self.get_mut(index).expect("dangling ptr")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut pool: LeakyPool<_> = LeakyPool::new();
        let ptr1 = pool.allocate(1);
        let ptr2 = pool.allocate(2);
        assert_eq!(pool[ptr1], 1);
        assert_eq!(pool[ptr2], 2);

        assert_eq!(pool.deallocate(ptr1), Some(1));
    }

    #[test]
    #[should_panic]
    fn dangling_ptr() {
        let mut pool: LeakyPool<_> = LeakyPool::new();
        let ptr = pool.allocate(1);
        pool.deallocate(ptr);
        pool[ptr];
    }

    #[test]
    fn wrong_pool() {
        let mut pool1 = LeakyPool::<u32>::new();
        let pool2 = LeakyPool::<u32>::new();
        let ptr = pool1.allocate(1);
        assert!(pool2.get(ptr).is_none());
    }
}
