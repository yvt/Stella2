//! The runtime components of TCW3 Designer.
//!
//! # `subscriber_list` Re-exports
//!
//! This crate re-exports items from `subscriber_list` so that the implementors
//! of Designer components do not have to depend on `subscriber_list` by
//! themselves.

#[doc(no_inline)]
pub use subscriber_list::{SubscriberList, UntypedSubscription as Sub};

/// A placeholder value for unset mandatory parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unset;
