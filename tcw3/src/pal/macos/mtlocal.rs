use std::{cell::UnsafeCell, fmt};

use super::{utils::is_main_thread, WM};

/// Like a recursive mutex, but only accessible by the main thread (MT).
pub struct MtLocal<T> {
    cell: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for MtLocal<T> {}
unsafe impl<T: Send> Send for MtLocal<T> {}

#[allow(dead_code)]
impl<T> MtLocal<T> {
    pub const fn new(x: T) -> Self {
        Self {
            cell: UnsafeCell::new(x),
        }
    }

    /// Get a reference to the inner value with run-time thread checking.
    ///
    /// Returns `Err(BadThread)` if the calling thread is not a main thread.
    pub fn try_get(&self) -> Result<&T, BadThread> {
        if is_main_thread() {
            Ok(unsafe { &*self.cell.get() })
        } else {
            Err(BadThread)
        }
    }

    /// Get a reference to the inner value with run-time thread checking.
    ///
    /// Panics if the calling thread is not a main thread.
    pub fn get(&self) -> &T {
        self.try_get().unwrap()
    }

    /// Get a reference to the inner value with compile-time thread checking.
    pub fn get_with_wm(&self, _: &WM) -> &T {
        unsafe { &*self.cell.get() }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.cell.get() }
    }
}

impl<T: fmt::Debug> fmt::Debug for MtLocal<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(x) = self.try_get() {
            f.debug_struct("MtLocal").field("value", &x).finish()
        } else {
            write!(f, "MtLocal {{ <inaccessible> }}")
        }
    }
}

/// Returned when a function/method is called from an invalid thread.
#[derive(Debug)]
pub struct BadThread;

impl std::fmt::Display for BadThread {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "the operation is invalid for the current thread.")
    }
}

impl std::error::Error for BadThread {}
