//! Reimplementation of the [atom] library with specialized and extended features.
//!
//! [atom]: https://crates.io/crates/atom
#![feature(box_into_raw_non_null)]
#![feature(const_fn)] // `const fn` with a constrained type parameter (e.g., `T: PtrSized`)
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Weak};
use std::{
    fmt, mem,
    ptr::{self, NonNull},
};

#[cfg(all(feature = "winrt", target_os = "windows"))]
mod winrt_comptr;

/// Types whose value can be converted into a non-zero pointer-sized value
/// and forth.
///
/// This trait is marked as `unsafe` because `from_raw` processes an
/// unvalidated pointer (which is supposed to be one returned by `into_raw`)
/// and the implementations must not panic.
pub unsafe trait PtrSized: Sized {
    /// Convert `Self` into a pointer.
    ///
    /// The returned pointer may be an invalid pointer (i.e. undereferenceable).
    fn into_raw(this: Self) -> NonNull<()>;

    /// Convert a pointer created by `into_raw` back to `Self`.
    unsafe fn from_raw(ptr: NonNull<()>) -> Self;
}

/// Types implementing `PtrSized` and having converted pointer values that can
/// be interpreted as safely-dereferenceable `*const Self::Target` .
///
/// This trait is marked as `unsafe` because it puts a restriction on the
/// implementation of `PtrSized`.
///
/// It's possible that some type can implement either of `TypedPtrSized` and
/// `TrivialPtrSized`, but not both of them. In such cases, prefer
/// `TypedPtrSized` because `TrivialPtrSized` can be implemented without a
/// knowledge about a specific type (TODO: add a wrapper type to demonstrate
/// this) while `TypedPtrSized` can't.
pub unsafe trait TypedPtrSized: PtrSized {
    type Target;
}

/// Types implementing `PtrSized` with a trivial implementation (i.e.,
/// conversion is done by mere transmutation).
///
/// This trait is marked as `unsafe` because it puts a restriction on the
/// implementation of `PtrSized`.
pub unsafe trait TrivialPtrSized: PtrSized {}

/// The pointed value is safe to mutate.
///
/// Types with `TypedPtrSized` usually implement this. However, there are
/// various reasons not to implement this; for example, they should not if the
/// deferenced value represents an internal state and must not be mutated.
/// `Arc` does not implement this because there may be other references to the
/// dereferenced value.
pub unsafe trait MutPtrSized: TypedPtrSized {}

trait PtrSizedExt: PtrSized {
    fn option_into_raw(this: Option<Self>) -> *mut ();
    unsafe fn option_from_raw(ptr: *mut ()) -> Option<Self>;
}

impl<T: PtrSized> PtrSizedExt for T {
    fn option_into_raw(this: Option<Self>) -> *mut () {
        if let Some(x) = this {
            Self::into_raw(x).as_ptr()
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn option_from_raw(ptr: *mut ()) -> Option<Self> {
        NonNull::new(ptr).map(|x| Self::from_raw(x))
    }
}

unsafe impl<T> PtrSized for Box<T> {
    fn into_raw(this: Self) -> NonNull<()> {
        unsafe { mem::transmute(Box::into_raw_non_null(this)) }
    }
    unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        Box::from_raw(ptr.as_ptr() as _)
    }
}
unsafe impl<T> TypedPtrSized for Box<T> {
    type Target = T;
}
unsafe impl<T> MutPtrSized for Box<T> {}
unsafe impl<T> TrivialPtrSized for Box<T> {}

unsafe impl<T> PtrSized for Arc<T> {
    fn into_raw(this: Self) -> NonNull<()> {
        NonNull::new(Arc::into_raw(this) as *mut ()).expect("pointer is unexpectedly null")
    }
    unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        Arc::from_raw(ptr.as_ptr() as _)
    }
}
unsafe impl<T> TypedPtrSized for Arc<T> {
    type Target = T;
}

unsafe impl<T> PtrSized for Weak<T> {
    fn into_raw(this: Self) -> NonNull<()> {
        unsafe { mem::transmute(this) }
    }
    unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        mem::transmute(ptr)
    }
}
unsafe impl<T> TrivialPtrSized for Weak<T> {}

/// An atomic `Option<Arc<T>>` storage that can be safely shared between threads.
pub struct Atom<T: PtrSized> {
    ptr: AtomicPtr<()>,
    phantom: PhantomData<T>,
}

unsafe impl<T: PtrSized + Sync> Sync for Atom<T> {}
unsafe impl<T: PtrSized + Send> Send for Atom<T> {}

impl<T: PtrSized> Atom<T> {
    /// Construct an empty `Atom`.
    pub fn empty() -> Self {
        Self {
            ptr: AtomicPtr::default(),
            phantom: PhantomData,
        }
    }

    /// Construct an `Atom` with an initial value.
    pub fn new(x: Option<T>) -> Self {
        Self {
            ptr: AtomicPtr::new(T::option_into_raw(x) as *mut ()),
            phantom: PhantomData,
        }
    }

    /// Return the inner object, consuming `self`.
    pub fn into_inner(mut self) -> Option<T> {
        let p = mem::replace(&mut self.ptr, AtomicPtr::default()).into_inner();

        // skip `drop`
        mem::forget(self);

        unsafe { T::option_from_raw(p) }
    }

    pub fn swap(&self, x: Option<T>, order: Ordering) -> Option<T> {
        let new_ptr = T::option_into_raw(x);
        let old_ptr = self.ptr.swap(new_ptr as *mut (), order);
        unsafe { T::option_from_raw(old_ptr) }
    }

    pub fn store(&self, x: Option<T>, order: Ordering) {
        self.swap(x, order);
    }

    pub fn take(&self, order: Ordering) -> Option<T> {
        self.swap(None, order)
    }
}

impl<T: PtrSized + Clone> Atom<T> {
    /// Clone the inner object of `Atom`, without (logically) modifying `self`.
    ///
    /// Note that this operation requires an unique reference to make the
    /// intermediate states (which is unsafe to manipulate) unobservable.
    pub fn load(&mut self) -> Option<T> {
        let ptr = self.ptr.get_mut();

        // Take
        let raw = *ptr;
        *ptr = ptr::null_mut();

        // Materialize and create a clone
        let obj = unsafe { T::option_from_raw(raw) };
        let obj2 = obj.clone();

        // Convert it back to a pointer
        let raw = T::option_into_raw(obj);
        *ptr = raw as *mut ();

        obj2
    }
}

impl<T: TrivialPtrSized> Atom<T> {
    /// Get a mutable reference to the inner object.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.ptr.get_mut().is_null() {
            None
        } else {
            Some(unsafe { &mut *((&mut self.ptr) as *mut _ as *mut T) })
        }
    }
}

impl<T: TypedPtrSized> Atom<T> {
    /// Derefernce the inner object.
    pub fn as_inner_ref(&mut self) -> Option<&T::Target> {
        let p = (*self.ptr.get_mut()) as *mut T::Target;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*p })
        }
    }

    /// Stores a value into the storage if the current value is the same as the
    /// `current` value.
    ///
    /// Returns the previous value with `Ok(x)` if the value was updated.
    /// `Err(new)` otherwise.
    pub fn compare_and_swap<P: AsRawPtr<T::Target>>(
        &self,
        current: &P,
        new: Option<T>,
        order: Ordering,
    ) -> Result<Option<T>, Option<T>> {
        let new_ptr = T::option_into_raw(new);
        let current_ptr = current.as_raw_ptr();
        let old_ptr = self
            .ptr
            .compare_and_swap(current_ptr as *mut (), new_ptr as *mut (), order);
        if old_ptr == current_ptr as *mut () {
            // Successful
            Ok(unsafe { T::option_from_raw(old_ptr) })
        } else {
            // Failure
            Err(unsafe { T::option_from_raw(new_ptr) })
        }
    }

    pub fn is_equal_to<P: AsRawPtr<T::Target>>(&self, other: &P, order: Ordering) -> bool {
        let other_ptr = other.as_raw_ptr();
        self.ptr.load(order) == other_ptr as *mut ()
    }
}

impl<T: TypedPtrSized + MutPtrSized> Atom<T> {
    /// Mutably dereference the inner object.
    pub fn as_inner_mut(&mut self) -> Option<&mut T::Target> {
        let p = (*self.ptr.get_mut()) as *mut T::Target;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &mut *p })
        }
    }
}

impl<T: PtrSized> fmt::Debug for Atom<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Atom").field(&self.ptr).finish()
    }
}

impl<T: PtrSized> Drop for Atom<T> {
    fn drop(&mut self) {
        self.take(Ordering::Relaxed);
    }
}

impl<T: PtrSized> Default for Atom<T> {
    fn default() -> Self {
        Atom::empty()
    }
}

pub trait AsRawPtr<T> {
    fn as_raw_ptr(&self) -> *const T;
}

impl<'a, T> AsRawPtr<T> for *const T {
    fn as_raw_ptr(&self) -> *const T {
        *self
    }
}

impl<'a, T> AsRawPtr<T> for &'a T {
    fn as_raw_ptr(&self) -> *const T {
        *self as *const _
    }
}

impl<'a, T> AsRawPtr<T> for &'a mut T {
    fn as_raw_ptr(&self) -> *const T {
        *self as *const _
    }
}

impl<T> AsRawPtr<T> for Arc<T> {
    fn as_raw_ptr(&self) -> *const T {
        &**self as *const _
    }
}

impl<T, S> AsRawPtr<T> for Option<S>
where
    S: AsRawPtr<T>,
{
    fn as_raw_ptr(&self) -> *const T {
        if let &Some(ref p) = self {
            p.as_raw_ptr()
        } else {
            ptr::null()
        }
    }
}

/// Like `Atom` but allows assignment only once throughout its lifetime.
#[derive(Default)]
pub struct SetOnceAtom<T: PtrSized> {
    ptr: AtomicPtr<()>,
    phantom: PhantomData<T>,
}

impl<T: PtrSized> SetOnceAtom<T> {
    /// Construct an empty `SetOnceAtom`.
    pub const fn empty() -> Self {
        Self {
            ptr: AtomicPtr::new(ptr::null_mut()),
            phantom: PhantomData,
        }
    }

    /// Construct a `SetOnceAtom`.
    pub fn new(x: Option<T>) -> Self {
        Self {
            ptr: AtomicPtr::new(T::option_into_raw(x) as *mut ()),
            phantom: PhantomData,
        }
    }

    /// Store a value if nothing is stored yet.
    ///
    /// Returns `Ok(())` if the operation was successful. Returns `Err(x)`
    /// if the cell was already occupied.
    pub fn store(&self, x: Option<T>) -> Result<(), Option<T>> {
        let new_ptr = T::option_into_raw(x);
        match self.ptr.compare_exchange(
            ptr::null_mut(),
            new_ptr as *mut _,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => Ok(()),
            Err(_) => Err(unsafe { T::option_from_raw(new_ptr) }),
        }
    }

    /// Return the inner object, consuming `self`.
    pub fn into_inner(mut self) -> Option<T> {
        let ret = unsafe { T::option_from_raw(*self.ptr.get_mut()) };

        // Skip drop
        mem::forget(self);

        ret
    }

    /// Remove and return the inner object.
    pub fn take(&mut self) -> Option<T> {
        let ret = mem::replace(self.ptr.get_mut(), ptr::null_mut());
        unsafe { T::option_from_raw(ret) }
    }
}

impl<T: TrivialPtrSized> SetOnceAtom<T> {
    /// Get a reference to the inner object.
    pub fn get(&self) -> Option<&T> {
        if self.ptr.load(Ordering::Acquire).is_null() {
            None
        } else {
            Some(unsafe { &*((&self.ptr) as *const _ as *const T) })
        }
    }

    /// Get a mutable reference to the inner object.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.ptr.get_mut().is_null() {
            None
        } else {
            Some(unsafe { &mut *((&mut self.ptr) as *mut _ as *mut T) })
        }
    }
}

impl<T: TypedPtrSized> SetOnceAtom<T> {
    /// Dereference the inner object.
    pub fn as_inner_ref(&self) -> Option<&T::Target> {
        let p = self.ptr.load(Ordering::Acquire) as *mut T::Target;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*p })
        }
    }
}

impl<T: TypedPtrSized + MutPtrSized> SetOnceAtom<T> {
    /// Mutably dereference the inner object.
    pub fn as_inner_mut(&mut self) -> Option<&mut T::Target> {
        let p = self.ptr.load(Ordering::Acquire) as *mut T::Target;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &mut *p })
        }
    }
}

impl<T: TypedPtrSized> fmt::Debug for SetOnceAtom<T>
where
    T::Target: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("SetOnceAtom")
            .field(&self.as_inner_ref())
            .finish()
    }
}

impl<T: PtrSized> Drop for SetOnceAtom<T> {
    fn drop(&mut self) {
        unsafe {
            T::option_from_raw(*self.ptr.get_mut());
        }
    }
}

/// Like `Atom` but allows assignment only once throughout its lifetime.
/// Not thread safe.
pub struct SetOnce<T: PtrSized> {
    ptr: Cell<*mut ()>,
    phantom: PhantomData<T>,
}

impl<T: PtrSized> Default for SetOnce<T> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T: PtrSized> SetOnce<T> {
    /// Construct an empty `SetOnce`.
    pub const fn empty() -> Self {
        Self {
            ptr: Cell::new(ptr::null_mut()),
            phantom: PhantomData,
        }
    }

    /// Construct a `SetOnce`.
    pub fn new(x: Option<T>) -> Self {
        Self {
            ptr: Cell::new(T::option_into_raw(x) as *mut ()),
            phantom: PhantomData,
        }
    }

    /// Store a value if nothing is stored yet.
    ///
    /// Returns `Ok(())` if the operation was successful. Returns `Err(x)`
    /// if the cell was already occupied.
    pub fn store(&self, x: Option<T>) -> Result<(), Option<T>> {
        let new_ptr = T::option_into_raw(x);
        if self.ptr.get().is_null() {
            self.ptr.set(new_ptr);
            Ok(())
        } else {
            Err(unsafe { T::option_from_raw(new_ptr) })
        }
    }

    /// Return the inner object, consuming `self`.
    pub fn into_inner(self) -> Option<T> {
        let ret = unsafe { T::option_from_raw(self.ptr.get()) };

        // Skip drop
        mem::forget(self);

        ret
    }

    /// Remove and return the inner object.
    pub fn take(&mut self) -> Option<T> {
        let ret = mem::replace(self.ptr.get_mut(), ptr::null_mut());
        unsafe { T::option_from_raw(ret) }
    }
}

impl<T: TrivialPtrSized> SetOnce<T> {
    /// Get a reference to the inner object.
    pub fn get(&self) -> Option<&T> {
        if self.ptr.get().is_null() {
            None
        } else {
            Some(unsafe { &*((&self.ptr) as *const _ as *const T) })
        }
    }

    /// Get a mutable reference to the inner object.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.ptr.get_mut().is_null() {
            None
        } else {
            Some(unsafe { &mut *((&mut self.ptr) as *mut _ as *mut T) })
        }
    }
}

impl<T: TypedPtrSized> SetOnce<T> {
    /// Dereference the inner object.
    pub fn as_inner_ref(&self) -> Option<&T::Target> {
        let p = self.ptr.get() as *mut T::Target;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*p })
        }
    }
}

impl<T: TypedPtrSized + MutPtrSized> SetOnce<T> {
    /// Mutably dereference the inner object.
    pub fn as_inner_mut(&mut self) -> Option<&mut T::Target> {
        let p = self.ptr.get() as *mut T::Target;
        if p.is_null() {
            None
        } else {
            Some(unsafe { &mut *p })
        }
    }
}

impl<T: TypedPtrSized> fmt::Debug for SetOnce<T>
where
    T::Target: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("SetOnce")
            .field(&self.as_inner_ref())
            .finish()
    }
}

impl<T: PtrSized> Drop for SetOnce<T> {
    fn drop(&mut self) {
        unsafe {
            T::option_from_raw(*self.ptr.get_mut());
        }
    }
}
