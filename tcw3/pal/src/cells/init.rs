use std::cell::{Cell, RefCell};

use crate::prelude::WmTrait;

/// Trait for types having a constant default value. This is essentially a
/// constant version of `Default`.
///
/// # Rationale
///
/// Marking default values as sendable is useful for safe default initialization
/// of `MtSticky<T>`, which is fulfilled by [`SendInit`]. The problem is that
/// `Default::default` is not `const fn`, making it unusable for static
/// initialization. This trait provides an alternative way to get default values
/// that is usable in constant contexts, sidestepping this problem.
pub trait Init {
    /// The default value.
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self;
}

/// Trait for types having [`Init::INIT`] that is sendable regardless of
/// their `Send`-ness.
pub unsafe trait SendInit: Init {}

// Empty by default
// --------------------------------------------------------------------------
// These container types are empty by default, and when empty, they are safe to
// send. Thus they are always `SendInit`.

impl<T> Init for Vec<T> {
    const INIT: Self = Self::new();
}
unsafe impl<T> SendInit for Vec<T> {}

impl<T: ?Sized> Init for neo_linked_list::LinkedListCell<T> {
    const INIT: Self = Self::new();
}
unsafe impl<T: ?Sized> SendInit for neo_linked_list::LinkedListCell<T> {}

impl<Element: 'static, Token> Init for leakypool::LeakyPool<Element, leakypool::LazyToken<Token>>
where
    leakypool::LazyToken<Token>: leakypool::TokenStore,
{
    const INIT: Self = Self::new();
}
unsafe impl<Element: 'static, Token> SendInit
    for leakypool::LeakyPool<Element, leakypool::LazyToken<Token>>
where
    leakypool::LazyToken<Token>: leakypool::TokenStore,
{
}

// Content is default-initialized
// --------------------------------------------------------------------------
// Cells and smart pointers have contents by default, so they inherit the
// `[Send]Init`-ness of their inner types.

impl<T: Init> Init for Cell<T> {
    const INIT: Self = Cell::new(T::INIT);
}
unsafe impl<T: SendInit> SendInit for Cell<T> {}

impl<T: Init> Init for RefCell<T> {
    const INIT: Self = RefCell::new(T::INIT);
}
unsafe impl<T: SendInit> SendInit for RefCell<T> {}

impl<T: Init> Init for neo_linked_list::AssertUnpin<T> {
    const INIT: Self = Self::new(T::INIT);
}
unsafe impl<T: SendInit> SendInit for neo_linked_list::AssertUnpin<T> {}

// Content is default-initialized and sent
// --------------------------------------------------------------------------
// `MtSticky` is a kind of cell, but logically sends the contents to a main
// thread, so it requires `SendInit` to implement `Init`.

impl<T: SendInit + 'static, TWM: WmTrait> Init for super::MtSticky<T, TWM> {
    const INIT: Self = unsafe { super::MtSticky::new_unchecked(T::INIT) };
}
unsafe impl<T: SendInit + 'static, TWM: WmTrait> SendInit for super::MtSticky<T, TWM> {}
