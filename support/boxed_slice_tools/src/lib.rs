//! Utilities for boxed slices.
//!
//! The functions in this crate generates better code than `std` equivalents
//! in general.

/// Constructs a boxed slice in a similar way to `vec!`.
///
/// Note: `boxed_slice![expr; n]` instantiates `n + 1` instances of the element
/// type unlike `vec!`.
///
/// # Examples
///
/// ```
/// use boxed_slice_tools::boxed_slice;
/// assert_eq!(*boxed_slice![42; 3], [42, 42, 42]);
/// assert_eq!(*boxed_slice![1, 2, 3], [1, 2, 3]);
/// ```
///
#[macro_export]
macro_rules! boxed_slice {
	($e:expr; $len:expr) => {
		$crate::repeating_by_clone(&$e, $len)
	};
	($($e:expr),*$(,)*) => {
		// `vec!` suffices in this case
		::std::vec![$($e),*].into_boxed_slice()
	};
}

/// Consturct a boxed slice using the given generator function.
///
/// # Examples
///
/// ```
/// use boxed_slice_tools::from_fn;
/// assert_eq!(*from_fn(|i| i * 2, 3), [0, 2, 4]);
/// ```
///
pub fn from_fn<T>(mut gen: impl FnMut(usize) -> T, len: usize) -> Box<[T]> {
    let mut v = Vec::<T>::with_capacity(len);
    debug_assert_eq!(v.capacity(), len);
    unsafe {
        for i in 0..len {
            v.as_mut_ptr().offset(i as isize).write(gen(i));
            v.set_len(i + 1);
        }
        v.set_len(v.capacity());
    }
    v.into_boxed_slice()
}

/// Construct a boxed slice by cloning the given prototype value.
///
/// # Examples
///
/// ```
/// use boxed_slice_tools::repeating_by_clone;
/// assert_eq!(*repeating_by_clone(&42, 3), [42, 42, 42]);
/// ```
///
pub fn repeating_by_clone<T: Clone>(proto: &T, len: usize) -> Box<[T]> {
    from_fn(|_| proto.clone(), len)
}

/// Construct a boxed slice by filling it with default values.
///
/// # Examples
///
/// ```
/// use boxed_slice_tools::repeating_default;
/// assert_eq!(*repeating_default::<u32>(3), [0, 0, 0]);
/// ```
///
pub fn repeating_default<T: Default>(len: usize) -> Box<[T]> {
    from_fn(|_| T::default(), len)
}
