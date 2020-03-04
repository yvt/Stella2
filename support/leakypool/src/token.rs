use quick_error::quick_error;
use std::{
    marker::PhantomData,
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// A counter-based unforgeable token used to access the contents of
/// a `TokenLock`.
///
/// It's implemented by a global monotonic counter, which will overflow if you
/// create an excessive number of tokens. The process will be terminated should
/// this occur.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct LeakyToken {
    i: NonZeroUsize,
}

impl Default for LeakyToken {
    fn default() -> Self {
        Self::new()
    }
}

impl LeakyToken {
    pub fn new() -> Self {
        static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(0);
        let i = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed) + 1;

        // If the highest bit is set, overflow is imminent, so terminate the
        // process
        if i > usize::max_value() / 2 {
            std::process::abort();
        }

        Self {
            i: NonZeroUsize::new(i + 1).unwrap(),
        }
    }

    pub fn id(&self) -> LeakyTokenId {
        LeakyTokenId { i: self.i }
    }
}

// Two distinct instances of `LeakyToken` never have an identical `i`, so this
// is safe
unsafe impl tokenlock::Token<LeakyTokenId> for LeakyToken {
    fn eq_id(&self, id: &LeakyTokenId) -> bool {
        self.i == id.i
    }
}

/// Token that cannot be used to access the contents of a `TokenLock`, but can
/// be used to create a new `TokenLock`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeakyTokenId {
    i: NonZeroUsize,
}

/// Trait for types wrapping `Token`, associated with `TokenId` they produce.
///
/// # Safety
///
/// Given `token_store1` and `token_store2`, each an instance of
/// `TokenStore`; `token`, an instance of  `Token` returned by
/// `token_store1.token_ref()` or `token_store1.token_mut()`;
/// and `id`, an instance of `TokenId` returned by `token_store2.id()`,
/// `Token::eq_id(&token, &id)` must return `true` if and only if `token_store1`
/// refers to the same instance as `token_store2` does. Note that the moments
/// at which these variables are evaluated are unspecified, meaning the
/// proposition must hold regardless of when these variables are evaluated. In
/// other words, the behavior must be consistent throughout the program's
/// lifetime.
pub unsafe trait TokenStore {
    type Token: tokenlock::Token<Self::TokenId>;
    type TokenId: 'static;

    fn token_ref(&self) -> &Self::Token;
    fn token_mut(&mut self) -> &mut Self::Token;
    fn id(&mut self) -> Self::TokenId;
}

unsafe impl TokenStore for LeakyToken {
    type Token = Self;
    type TokenId = LeakyTokenId;

    fn token_ref(&self) -> &Self::Token {
        self
    }
    fn token_mut(&mut self) -> &mut Self::Token {
        self
    }
    fn id(&mut self) -> Self::TokenId {
        LeakyToken::id(self)
    }
}

// ----------------------------------------------------------------------------
// Lazy wrapper

/// An implementation of [`TokenStore`] that creates the inner token lazily.
///
/// `Inner` is expected to be `TokenStore<Token = Self>`.
///
/// The intention of this type is to allow the construction of `LeakyPool` in a
/// constant context.
#[derive(Debug)]
pub struct LazyToken<Inner> {
    inner: Option<Inner>,
}

impl<Inner> Default for LazyToken<Inner> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<Inner: tokenlock::Token<I>, I> tokenlock::Token<I> for LazyToken<Inner> {
    fn eq_id(&self, id: &I) -> bool {
        if let Some(inner) = &self.inner {
            inner.eq_id(id)
        } else {
            // `self.inner` is not initialized yet, so it equates to no
            // instance of `I` for now
            false
        }
    }
}

impl<Inner> LazyToken<Inner> {
    pub const fn new() -> Self {
        Self { inner: None }
    }

    /// A cold function wrapper of `Inner::default`.
    #[cold]
    fn new_inner() -> Inner
    where
        Inner: Default,
    {
        Inner::default()
    }
}

unsafe impl<Inner> TokenStore for LazyToken<Inner>
where
    Inner: Default + TokenStore + tokenlock::Token<<Inner as TokenStore>::TokenId>,
{
    type Token = Self;
    type TokenId = Inner::TokenId;

    fn token_ref(&self) -> &Self::Token {
        self
    }
    fn token_mut(&mut self) -> &mut Self::Token {
        self
    }
    fn id(&mut self) -> Self::TokenId {
        self.inner.get_or_insert_with(Self::new_inner).id()
    }
}

// ----------------------------------------------------------------------------
// Unchecked token

/// An implementation of [`TokenStore`] that does not perform runtime checks.
#[derive(Debug)]
pub struct UncheckedToken {
    _ctor_is_unsafe: (),
}

impl UncheckedToken {
    /// Construct an `UncheckedToken`.
    pub const unsafe fn new() -> Self {
        Self {
            _ctor_is_unsafe: (),
        }
    }
}

unsafe impl tokenlock::Token<()> for UncheckedToken {
    fn eq_id(&self, _: &()) -> bool {
        true
    }
}

unsafe impl TokenStore for UncheckedToken {
    type Token = Self;
    type TokenId = ();

    fn token_ref(&self) -> &Self::Token {
        self
    }
    fn token_mut(&mut self) -> &mut Self::Token {
        self
    }
    fn id(&mut self) -> Self::TokenId {
        ()
    }
}

// ----------------------------------------------------------------------------
// Singleton token

/// This is an implementation detail of [`singleton_tag!`].
pub unsafe trait SingletonTag: 'static {
    /// Get a reference to the flag variable indicating whether the token with
    /// this tag has already been created or not.
    #[doc(hidden)]
    fn usage_flag() -> &'static AtomicBool;
}

/// Defines a tag type for a singleton token.
///
/// # Examples
///
///     use leakypool::{
///         LeakyPool, PoolPtr, SingletonToken, SingletonTokenId, LazyToken,
///     };
///
///     leakypool::singleton_tag!(struct Tag);
///     type Pool<Element> = LeakyPool<Element, LazyToken<SingletonToken<Tag>>>;
///     type Ptr<Element> = PoolPtr<Element, SingletonTokenId<Tag>>;
///
///     // Since `LazyToken` is used here, `SingletonToken<Tag>` is created by
///     // `SingletonToken::<Tag>::default()` when the first element is inserted
///     let mut pool: Pool<u32> = Pool::new();
///     let ptr: Ptr<u32> = pool.allocate(1);
///     assert_eq!(pool[ptr], 1);
///
///     // Subsequent creation will fail because there can be only one
///     // instance of `SingletonToken<Tag>`
///     assert!(SingletonToken::<Tag>::try_new().is_err());
///
#[macro_export]
macro_rules! singleton_tag {
    {
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $(;)?
    } => {
        $(#[$meta])*
        #[derive(Debug)]
        $vis struct $name;

        unsafe impl $crate::SingletonTag for $name {
            fn usage_flag() -> &'static ::std::sync::atomic::AtomicBool {
                static FLAG: ::std::sync::atomic::AtomicBool =
                    ::std::sync::atomic::AtomicBool::new(false);
                &FLAG
            }
        }
    };
}

/// An implementation of [`TokenStore`] as a singleton object.
///
/// The constructor ensures there are no existing instances of the token using
/// a global flag variable.
///
/// Since there can be only one instance of `SingletonToken<Tag>`, the token
/// is zero-sized and incurs zero runtime overhead for token comparisons.
#[derive(Debug)]
pub struct SingletonToken<Tag> {
    _tag: PhantomData<Tag>,
    _ctor_is_private: (),
}

/// The `TokenId` type of [`SingletonToken`].
#[derive(Debug)]
pub struct SingletonTokenId<Tag> {
    _tag: PhantomData<Tag>,
    _ctor_is_private: (),
}

quick_error! {
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub enum NewSingletonTokenError {
        /// The token cannot be constructed because there is already one.
        AlreadyInstantiated {}
    }
}

impl<Tag: SingletonTag> Default for SingletonToken<Tag> {
    /// Calls `Self::new`.
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<Tag> SingletonToken<Tag> {
    /// Construct an `SingletonToken` without checking the uniqueness.
    pub unsafe fn new_unchecked() -> Self {
        Self {
            _tag: PhantomData,
            _ctor_is_private: (),
        }
    }
}

impl<Tag: SingletonTag> SingletonToken<Tag> {
    /// Construct an `SingletonToken`. Panics if there already is an instance
    /// of `SingletonToken<Tag>`.
    #[inline]
    pub fn new() -> Self {
        // Since this function is marked as `#[cold]`, this code will probably
        // be compiled into `lea` + `call`
        singleton_set_usage_flag(Tag::usage_flag());
        unsafe { Self::new_unchecked() }
    }

    /// Construct an `SingletonToken`.
    ///
    /// Prefer using `new` for code size optimization.
    pub fn try_new() -> Result<Self, NewSingletonTokenError> {
        singleton_set_usage_flag_inner(Tag::usage_flag())?;
        Ok(unsafe { Self::new_unchecked() })
    }
}

#[cold]
#[inline(never)]
fn singleton_set_usage_flag(flag: &AtomicBool) {
    singleton_set_usage_flag_inner(flag).unwrap();
}

fn singleton_set_usage_flag_inner(flag: &AtomicBool) -> Result<(), NewSingletonTokenError> {
    if flag.swap(true, Ordering::Relaxed) {
        Err(NewSingletonTokenError::AlreadyInstantiated)
    } else {
        Ok(())
    }
}

unsafe impl<Tag: 'static> tokenlock::Token<SingletonTokenId<Tag>> for SingletonToken<Tag> {
    fn eq_id(&self, _: &SingletonTokenId<Tag>) -> bool {
        true
    }
}

// This is safe because for each `Tag` there is up to only one instance of
// `SingletonToken<Tag>`.
unsafe impl<Tag: 'static> TokenStore for SingletonToken<Tag> {
    type Token = Self;
    type TokenId = SingletonTokenId<Tag>;

    fn token_ref(&self) -> &Self::Token {
        self
    }
    fn token_mut(&mut self) -> &mut Self::Token {
        self
    }
    fn id(&mut self) -> Self::TokenId {
        SingletonTokenId {
            _tag: PhantomData,
            _ctor_is_private: (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchecked() {
        use tokenlock::Token;
        let mut token = unsafe { UncheckedToken::new() };
        let id = token.id();
        assert!(token.token_ref().eq_id(&id));
        assert!(token.token_mut().eq_id(&id));
    }
}
