//! A state management library implementing the principles of
//! [Redux](https://redux.js.org) in Rust.
//!
//!  - *Elements* ([`Elem`]) store state data, updated through *reducers*.
//!
//! # Usage
//!
//!     use harmony::{Elem, set_field};
//!
//!     #[derive(Clone)]
//!     struct App {
//!         primitive_field: f32,
//!         main_wnd: Elem<Wnd>,
//!     }
//!     #[derive(Clone)]
//!     struct Wnd {
//!         count: usize,
//!     }
//!
//!     enum Action {
//!         SetPrimitive(f32),
//!         Wnd(WndAction),
//!     }
//!     enum WndAction {
//!         Increment,
//!     }
//!
//!     impl App {
//!         fn reduce(this: Elem<Self>, act: &Action) -> Elem<Self> {
//!             match act {
//!                 Action::SetPrimitive(x) => set_field! {
//!                     primitive_field: *x,
//!                     ..this
//!                 },
//!                 Action::Wnd(wnd_act) => set_field! {
//!                     main_wnd: Wnd::reduce(this.main_wnd.clone(), wnd_act),
//!                     ..this
//!                 },
//!             }
//!         }
//!     }
//!     impl Wnd {
//!         fn reduce(this: Elem<Self>, act: &WndAction) -> Elem<Self> {
//!             match act {
//!                 WndAction::Increment => set_field! {
//!                     count: this.count + 1,
//!                     ..this
//!                 },
//!             }
//!         }
//!     }
//!
//!     let state = Elem::new(App {
//!         primitive_field: 1.0,
//!         main_wnd: Elem::new(Wnd { count: 1 }),
//!     });
//!     let state = App::reduce(state, &Action::Wnd(WndAction::Increment));
//!     assert_eq!(state.main_wnd.count, 2);
//!
#![feature(specialization)]
use std::{fmt, rc::Rc};

#[cfg(feature = "miniserde")]
mod miniserde;

/// A container type for state data.
///
/// `Elem` is conceptually immutable, but may perform in-place mutation when
/// there are no other owners.
#[derive(Debug, Clone)]
pub struct Elem<T: ?Sized> {
    inner: Rc<T>,
}

impl<T> Elem<T> {
    /// Construct a `Elem` with the specified inner value.
    pub fn new(x: T) -> Self {
        Self { inner: Rc::new(x) }
    }

    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        Rc::ptr_eq(&this.inner, &other.inner)
    }
}

impl<T: Clone> Elem<T> {
    #[doc(hidden)]
    pub fn make_mut(&mut self) -> &mut T {
        Rc::make_mut(&mut self.inner)
    }
}

impl<T: ?Sized> std::ops::Deref for Elem<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<T> From<T> for Elem<T> {
    fn from(x: T) -> Self {
        Self::new(x)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for Elem<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&*self.inner, f)
    }
}

/// Update one or more fields of the object contained by [`Elem`], creating a
/// new `Elem`. It consumes the input `Elem`.
///
/// This macro automatically employs the following optimization:
///
///  - It compares the new field values against the old ones using
///    [`ShallowEq`], and it simply returns the original `Elem` if they are
///    identical.
///  - If there are no other references to the input `Elem`'s underlying
///    storage, this macro mutates it without copying.
///
#[macro_export]
macro_rules! set_field {
    (
        $($field:ident : $value:expr ,)*
        .. $in_elem:expr
    ) => {{
        $(
            let $field = $value;
        )*
        let mut in_elem: $crate::Elem<_> = $in_elem;
        if false || $( $crate::ShallowEq::shallow_ne(&$field, &in_elem.$field) )* {
            let inner = $crate::Elem::make_mut(&mut in_elem);
            $( inner.$field = $field; )*
        }
        in_elem
    }};
}

/// Similar to `PartialEq`, but may perform shallow comparison and incorrectly
/// output "not equal" for deep structures with identical children
/// which are logically identical, but located in different memory locations.
pub trait ShallowEq {
    #[must_use]
    fn shallow_eq(&self, other: &Self) -> bool;

    #[must_use]
    fn shallow_ne(&self, other: &Self) -> bool {
        !self.shallow_eq(other)
    }
}

impl<T> ShallowEq for T
where
    T: PartialEq,
{
    default fn shallow_eq(&self, other: &Self) -> bool {
        *self == *other
    }
    default fn shallow_ne(&self, other: &Self) -> bool {
        *self != *other
    }
}

impl<T> ShallowEq for Elem<T> {
    fn shallow_eq(&self, other: &Self) -> bool {
        Self::ptr_eq(self, other)
    }
    fn shallow_ne(&self, other: &Self) -> bool {
        !Self::ptr_eq(self, other)
    }
}
