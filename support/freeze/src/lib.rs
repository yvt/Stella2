use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fmt, ops};

/// A thread-safe cell type supporting the "freezing" operation.
pub struct FreezableCell<T> {
    data: UnsafeCell<T>,
    state: AtomicUsize,
}

/// A lock guard of [`FreezableCell`] for mutable access.
pub struct FreezableCellRef<'a, T: 'a>(&'a FreezableCell<T>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreezableCellFreezeError {
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreezableCellLockError {
    Locked,
    Frozen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreezableCellBorrowError {
    Unfrozen,
}

const STATE_UNFROZEN: usize = 0;
const STATE_UNFROZEN_LOCKED: usize = 1;
const STATE_FROZEN: usize = 2;

unsafe impl<T: Sync> Sync for FreezableCell<T> {}

impl<T: fmt::Debug> fmt::Debug for FreezableCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(x) = self.frozen_borrow() {
            f.debug_tuple("FreezableCell").field(x).finish()
        } else {
            write!(f, "FreezableCell([borrowed])")
        }
    }
}

impl<T> FreezableCell<T> {
    /// Construct a `FreezableCell`.
    ///
    /// The created cell will be initially in the unfrozen state.
    pub const fn new_unfrozen(x: T) -> Self {
        Self {
            data: UnsafeCell::new(x),
            state: AtomicUsize::new(STATE_UNFROZEN),
        }
    }

    /// Construct a `FreezableCell`.
    ///
    /// The created cell will be initially in the frozen state.
    pub const fn new_frozen(x: T) -> Self {
        Self {
            data: UnsafeCell::new(x),
            state: AtomicUsize::new(STATE_FROZEN),
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }

    /// Freeze the cell.
    ///
    /// This is an irreversible operation.
    pub fn freeze(&self) -> Result<(), FreezableCellFreezeError> {
        match self.state.compare_exchange(
            STATE_UNFROZEN,
            STATE_FROZEN,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => Ok(()),
            Err(x) => {
                if x == STATE_UNFROZEN_LOCKED {
                    Err(FreezableCellFreezeError::Locked)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Obtain a mutable reference to the contents of a cell in the unfrozen
    /// state.
    pub fn unfrozen_borrow_mut(&self) -> Result<FreezableCellRef<'_, T>, FreezableCellLockError> {
        match self.state.compare_exchange(
            STATE_UNFROZEN,
            STATE_UNFROZEN_LOCKED,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => Ok(FreezableCellRef(self)),
            Err(x) => {
                if x == STATE_UNFROZEN_LOCKED {
                    Err(FreezableCellLockError::Locked)
                } else {
                    Err(FreezableCellLockError::Frozen)
                }
            }
        }
    }

    /// Obtain a reference to the contents of a cell in the frozen state.
    pub fn frozen_borrow(&self) -> Result<&T, FreezableCellBorrowError> {
        if self.state.load(Ordering::Acquire) == STATE_FROZEN {
            Ok(unsafe { &*self.data.get() })
        } else {
            Err(FreezableCellBorrowError::Unfrozen)
        }
    }
}

impl<'a, T> FreezableCellRef<'a, T> {
    pub fn freeze(this: Self) {
        use std::mem::forget;
        this.0.state.store(STATE_FROZEN, Ordering::Release);
        forget(this);
    }
}

impl<'a, T> Drop for FreezableCellRef<'a, T> {
    fn drop(&mut self) {
        self.0.state.store(STATE_UNFROZEN, Ordering::Release);
    }
}

impl<'a, T> ops::Deref for FreezableCellRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.0.data.get() }
    }
}

impl<'a, T> ops::DerefMut for FreezableCellRef<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.0.data.get() }
    }
}
