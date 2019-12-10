//! The runtime components of TCW3 Designer.
//!
//! # Re-exports
//!
//! This crate re-exports items from some crates so that the implementors
//! of Designer components do not have to depend on `subscriber_list` by
//! themselves.

#[doc(no_inline)]
pub use subscriber_list::{SubscriberList, UntypedSubscription as Sub};

#[doc(no_inline)]
pub use owning_ref::OwningRef;

/// A placeholder value for unset mandatory parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unset;
