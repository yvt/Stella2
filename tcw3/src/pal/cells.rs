use std::{cell::UnsafeCell, mem::ManuallyDrop};

use super::{iface::WM as _, WM};

/// Main-Thread Sticky — Like [`fragile::Sticky`], allows `!Send` types to be
/// moved between threads, but there are a few differences:
///
///  - The ownership is restricted to the main thread.
///  - When dropped, the inner value is sent back to the main thread and
///    destroyed in the main event loop.
///  - Provides additional methods for compile-time thread checking.
///
/// [`fragile::Sticky`]: https://docs.rs/fragile/0.3.0/fragile/struct.Sticky.html
pub struct MtSticky<T: 'static> {
    cell: ManuallyDrop<UnsafeCell<T>>,
}

unsafe impl<T: 'static> Send for MtSticky<T> {}
unsafe impl<T: 'static + Sync> Sync for MtSticky<T> {}

#[allow(dead_code)]
impl<T: 'static> MtSticky<T> {
    /// Construct a `MtSticky` without thread checking.
    #[inline]
    pub const unsafe fn new_unchecked(x: T) -> Self {
        Self {
            cell: ManuallyDrop::new(UnsafeCell::new(x)),
        }
    }

    /// Construct a `MtSticky` containing a `Send`-able value.
    #[inline]
    pub const fn new(x: T) -> Self
    where
        T: Send,
    {
        unsafe { Self::new_unchecked(x) }
    }

    /// Construct a `MtSticky` with compile-time thread checking.
    #[inline]
    pub fn with_wm(_: WM, x: T) -> Self {
        unsafe { Self::new_unchecked(x) }
    }

    /// Get a raw pointer to the inner value.
    #[inline]
    pub fn get_ptr(&self) -> *mut T {
        self.cell.get()
    }

    /// Take the inner value with run-time thread checking.
    #[inline]
    pub fn into_inner(self, _: WM) -> T {
        let inner = unsafe { self.cell.get().read() };
        std::mem::forget(self);
        inner
    }

    /// Get a reference to the `Send`-able and `Sync` inner value.
    #[inline]
    pub fn get(&self) -> &T
    where
        T: Send + Sync,
    {
        unsafe { &*self.get_ptr() }
    }

    /// Get a reference to the `Send`-able inner value
    #[inline]
    pub fn get_mut(&mut self) -> &mut T
    where
        T: Send,
    {
        unsafe { &mut *self.get_ptr() }
    }

    /// Get a reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_with_wm(&self, _: &WM) -> &T {
        unsafe { &*self.get_ptr() }
    }

    /// Get a mutable reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_mut_with_wm(&mut self, _: &WM) -> &mut T {
        unsafe { &mut *self.get_ptr() }
    }
}

impl<T: 'static> Drop for MtSticky<T> {
    fn drop(&mut self) {
        if std::mem::needs_drop::<T>() {
            struct AssertSend<T>(T);
            unsafe impl<T> Send for AssertSend<T> {}

            // This is safe because the inner value was originally created
            // in the main thread, and we are sending it back to the main
            // thread.
            let cell = AssertSend(unsafe { self.cell.get().read() });
            WM::invoke_on_main_thread(move |_| {
                drop(cell);
            });
        }
    }
}

/// Main-Thread Lock — Like `ReentrantMutex`, but only accessible to the main thread.
pub struct MtLock<T> {
    cell: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for MtLock<T> {}
unsafe impl<T: Send> Sync for MtLock<T> {}

#[allow(dead_code)]
impl<T> MtLock<T> {
    /// Construct a `MtLock`.
    #[inline]
    pub const fn new(x: T) -> Self {
        Self {
            cell: UnsafeCell::new(x),
        }
    }

    /// Get a raw pointer to the inner value.
    #[inline]
    pub const fn get_ptr(&self) -> *mut T {
        self.cell.get()
    }

    /// Take the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.cell.into_inner()
    }

    /// Get a reference to the `Sync` inner value.
    #[inline]
    pub fn get(&self) -> &T
    where
        T: Sync,
    {
        unsafe { &*self.get_ptr() }
    }

    /// Get a mutably reference to the inner value.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.get_ptr() }
    }

    /// Get a reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_with_wm(&self, _: &WM) -> &T {
        unsafe { &*self.get_ptr() }
    }
}
