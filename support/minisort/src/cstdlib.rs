//! The safe wrapper of `qsort`.
use std::{
    cmp::Ordering,
    mem::{size_of, MaybeUninit},
    os::raw::{c_int, c_void},
    ptr::NonNull,
    sync::{
        atomic::{AtomicUsize, Ordering as AOrdering},
        Mutex, MutexGuard, Once,
    },
};

/// Sort the slice using the `qsort` function from the C standard library.
///
/// # Performance
///
/// It was faster than [`insertion_sort`] for `a.len() > 128`. It was slower
/// than `[T]::sort_unstable` in general by a constant factor.
///
/// [`insertion_sort`]: crate::insertion_sort
///
/// # Examples
///
/// ```
/// let mut v = [-5, 4, 1, -3, 2];
///
/// minisort::qsort(&mut v);
/// assert!(v == [-5, -3, 1, 2, 4]);
/// ```
pub fn qsort<T: Ord>(a: &mut [T]) {
    qsort_raw(a, |x, y| x.qcmp(y));
}

/// Sort the slice with a key extraction function.
///
/// It's optimized for when `K` is a primitive type. `f` should be zero-sized
/// for optimal performance. This is because `libc::qsort` doesn't passing
/// context data.
///
/// # Examples
///
/// ```
/// let mut v = [-5i32, 4, 1, -3, 2];
///
/// minisort::qsort_by_key(&mut v, |k| k.abs());
/// assert!(v == [1, 2, -3, 4, -5]);
/// ```
pub fn qsort_by_key<T, K: Ord>(a: &mut [T], mut f: impl FnMut(&T) -> K) {
    qsort_raw(a, |x, y| f(x).qcmp(&f(y)));
}

/// Sort the slice with a comparator function.
///
/// `f` should be zero-sized for optimal performance. This is because
/// `libc::qsort` doesn't passing context data.
///
/// # Examples
///
/// ```
/// let mut v = [5, 4, 1, 3, 2];
/// minisort::qsort_by(&mut v, |a, b| a.cmp(b));
/// assert!(v == [1, 2, 3, 4, 5]);
/// ```
pub fn qsort_by<T>(a: &mut [T], mut f: impl FnMut(&T, &T) -> Ordering) {
    qsort_raw(a, |x, y| to_qsort_ordering(f(x, y)));
}

pub fn qsort_raw<T, F: FnMut(&T, &T) -> c_int>(a: &mut [T], mut f: F) {
    if size_of::<F>() > 0 {
        // `qsort` doesn't support passing a context pointer. We emulate one
        // by a global variable.
        static CTX: AtomicUsize = AtomicUsize::new(0);
        static mut MUTEX: MaybeUninit<Mutex<()>> = MaybeUninit::uninit();
        static MUTEX_ONCE: Once = Once::new();

        fn lock() -> MutexGuard<'static, ()> {
            // Initialize `MUTEX` only once. Do this in a non-generic function
            // for code size saving
            MUTEX_ONCE.call_once(|| unsafe {
                MUTEX = MaybeUninit::new(Mutex::new(()));
            });

            let mutex: &'static Mutex<()> = unsafe { &*MUTEX.as_ptr() };
            mutex.lock().unwrap_or_else(|e| e.into_inner())
        }

        // Acquire an exclusive ownership of `CTX`
        let _guard = lock();
        CTX.store((&mut f) as *mut _ as usize, AOrdering::Relaxed);

        unsafe extern "C" fn contextful_sorter<T, F: FnMut(&T, &T) -> c_int>(
            x: *const c_void,
            y: *const c_void,
        ) -> c_int {
            let ctx: &mut F = &mut *(CTX.load(AOrdering::Relaxed) as *mut F);
            ctx(&*(x as *const T), &*(y as *const T))
        }

        unsafe {
            libc::qsort(
                a.as_mut_ptr() as _,
                a.len(),
                std::mem::size_of::<T>(),
                Some(contextful_sorter::<T, F>),
            );
        }
    } else {
        unsafe extern "C" fn contextless_sorter<T, F: FnMut(&T, &T) -> c_int>(
            x: *const c_void,
            y: *const c_void,
        ) -> c_int {
            let mut ctx: NonNull<F> = NonNull::dangling();
            ctx.as_mut()(&*(x as *const T), &*(y as *const T))
        }

        unsafe {
            libc::qsort(
                a.as_mut_ptr() as _,
                a.len(),
                std::mem::size_of::<T>(),
                Some(contextless_sorter::<T, F>),
            );
        }
    }
}

pub fn to_qsort_ordering(o: Ordering) -> c_int {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// A version of `Ord` which is nice for `qemu`.
trait QOrd {
    fn qcmp(&self, other: &Self) -> c_int;
}

impl<T: Ord> QOrd for T {
    #[inline]
    default fn qcmp(&self, other: &Self) -> c_int {
        to_qsort_ordering(self.cmp(other))
    }
}

macro_rules! impl_qord {
    (@signed $($t:ty),*) => {$(
        impl QOrd for $t {
            #[inline]
            fn qcmp(&self, other: &Self) -> c_int {
                if size_of::<$t>() > size_of::<c_int>() {
                    to_qsort_ordering(self.cmp(other))
                } else if size_of::<$t>() == size_of::<c_int>() {
                    self.saturating_sub(*other) as c_int
                } else {
                    *self as c_int - *other as c_int
                }
            }
        }
    )*};

    (@unsigned $($t:ty),*) => {$(
        impl QOrd for $t {
            #[inline]
            fn qcmp(&self, other: &Self) -> c_int {
                if size_of::<$t>() >= size_of::<c_int>() {
                    to_qsort_ordering(self.cmp(other))
                } else {
                    *self as c_int - *other as c_int
                }
            }
        }
    )*};
}

impl_qord!(@signed i8, i16, i32, i64, i128);
impl_qord!(@unsigned u8, u16, u32, u64, u128);

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    macro_rules! gen_tests {
        ($($t:tt),*) => {$(
            mod $t {
                use super::*;

                #[quickcheck]
                fn result_is_sorted(mut v: Vec<$t>) -> bool {
                    qsort(&mut v);
                    v.is_sorted()
                }

                #[quickcheck]
                fn result_is_sorted_by_key(mut v: Vec<($t, $t)>) -> bool {
                    qsort_by_key(&mut v, |e| e.1);
                    v.is_sorted_by_key(|e| e.1)
                }

                #[quickcheck]
                fn result_is_sorted_by(mut v: Vec<$t>) -> bool {
                    qsort_by(&mut v, |x, y| y.cmp(x));
                    v.is_sorted_by(|x, y| Some(y.cmp(x)))
                }
            }
        )*};
    }

    gen_tests!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, bool, char);
}
