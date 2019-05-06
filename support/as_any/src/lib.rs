//! Blanket trait implementations for types which are `Sized + Any`, allowing
//! conversion to `&dyn Any`.
//!
//!     use as_any::{AsAny, Downcast};
//!     use std::any::Any;
//!
//!     trait MyTrait: AsAny {}
//!     impl MyTrait for i32 {}
//!
//!     let x: Box<dyn MyTrait> = Box::new(42i32);
//!
//!     // Usually, you can't cast `&dyn MyTrait` into `&dyn Any`:
//!     // let x_any: &dyn Any = &*x;
//!
//!     // `AsAny` makes it possible:
//!     let x_any = (*x).as_any();
//!
//!     assert_eq!((*x).downcast_ref(), Some(&42i32));
//!
use std::any::Any;

/// Allows conversion from a reference to `&dyn Any`.
pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Allows conversion from a reference to `&(dyn Any + Send)`.
pub trait AsAnySend: AsAny + Send {
    fn as_any_send(&self) -> &(dyn Any + Send);
    fn as_any_send_mut(&mut self) -> &mut (dyn Any + Send);
}

/// Allows conversion from a reference to `&(dyn Any + Send + Sync)`.
pub trait AsAnySendSync: AsAny + Send + Sync {
    fn as_any_send_sync(&self) -> &(dyn Any + Send + Sync);
    fn as_any_send_sync_mut(&mut self) -> &mut (dyn Any + Send + Sync);
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<T: Any + Send> AsAnySend for T {
    fn as_any_send(&self) -> &(dyn Any + Send) {
        self
    }
    fn as_any_send_mut(&mut self) -> &mut (dyn Any + Send) {
        self
    }
}

impl<T: Any + Send + Sync> AsAnySendSync for T {
    fn as_any_send_sync(&self) -> &(dyn Any + Send + Sync) {
        self
    }
    fn as_any_send_sync_mut(&mut self) -> &mut (dyn Any + Send + Sync) {
        self
    }
}

/// An extension trait for [`AsAny`] that provides downcasting methods.
pub trait Downcast: AsAny {
    /// Returns `true` if the underlying type is identical with `T`.
    fn is<T: Any>(&self) -> bool;
    /// Attempt a downcast. Returns a reference to a concrete type if
    /// successful.
    fn downcast_ref<T: Any>(&self) -> Option<&T>;
    /// Attempt a downcast. Returns a mutable reference to a concrete type if
    /// successful.
    fn downcast_mut<T: Any>(&mut self) -> Option<&mut T>;
}

impl<S: AsAny + ?Sized> Downcast for S {
    fn is<T: Any>(&self) -> bool {
        self.as_any().is::<T>()
    }
    fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.as_any().downcast_ref()
    }
    fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut()
    }
}
