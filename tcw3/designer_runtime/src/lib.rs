//! The runtime components of TCW3 Designer. It's by no means intended to be
//! used by hand-written code.
//!
//! # Re-exports
//!
//! This crate re-exports items from some crates so that the implementors
//! of Designer components do not have to depend on `subscriber_list` by
//! themselves.
use std::{cell::Cell, mem::MaybeUninit};
use tcw3_pal as pal;
use tcw3_pal::prelude::*;

#[doc(no_inline)]
pub use subscriber_list::{SubscriberList, UntypedSubscription as Sub};

#[doc(no_inline)]
pub use owning_ref::OwningRef;

#[doc(no_inline)]
pub use harmony::ShallowEq;

/// A placeholder value for unset mandatory parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unset;

/// Unwrap a `Option<T>`. Does not check if it is `Some(_)` unless debug
/// assertions are enabled.
///
/// # Safety
///
/// `x` must be `Some(_)`.
#[inline]
pub unsafe fn unwrap_unchecked<T>(x: Option<T>) -> T {
    debug_assert!(x.is_some(), "attempted to unwrap a None value");
    x.unwrap_or_else(|| std::hint::unreachable_unchecked())
}

/// Take the ownership of all `Sub` in `subs` and unsubscribe them later
/// using `Wm::invoke`.
///
/// # Safety
///
/// All elements of `subs` must be in an initialized state.
pub unsafe fn unsubscribe_subs_unchecked(wm: pal::Wm, subs: &mut [Cell<MaybeUninit<Sub>>]) {
    // This requires only once allocation (because the size is pre-known) and
    // takes less space than `Vec`
    let mut subs: Box<[_]> = subs
        .iter_mut()
        .map(|s| std::mem::replace(s, Cell::new(MaybeUninit::uninit())))
        .collect();

    // Assumes this closure will be called eventually.
    wm.invoke(move |_| {
        for sub in subs.iter_mut() {
            let sub = sub.get_mut().as_mut_ptr().read();
            sub.unsubscribe().unwrap();
        }
    });
}
