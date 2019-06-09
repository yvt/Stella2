use std::{cell::UnsafeCell, fmt, marker::PhantomData, mem::ManuallyDrop};

use super::{iface::WM as WMTrait, WM};

/// Main-Thread Sticky — Like [`fragile::Sticky`], allows `!Send` types to be
/// moved between threads, but there are a few differences:
///
///  - The ownership is restricted to the main thread.
///  - When dropped, the inner value is sent back to the main thread and
///    destroyed in the main event loop.
///  - Provides additional methods for compile-time thread checking.
///
/// [`fragile::Sticky`]: https://docs.rs/fragile/0.3.0/fragile/struct.Sticky.html
pub struct MtSticky<T: 'static, TWM: WMTrait = WM> {
    _phantom: PhantomData<TWM>,
    cell: ManuallyDrop<UnsafeCell<T>>,
}

unsafe impl<T: 'static, TWM: WMTrait> Send for MtSticky<T, TWM> {}
unsafe impl<T: 'static, TWM: WMTrait> Sync for MtSticky<T, TWM> {}

impl<T: 'static + fmt::Debug, TWM: WMTrait> fmt::Debug for MtSticky<T, TWM> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(wm) = WM::try_global() {
            f.debug_tuple("MtSticky")
                .field(self.get_with_wm(wm))
                .finish()
        } else {
            write!(f, "MtSticky(<not main thread>)")
        }
    }
}

#[allow(dead_code)]
impl<T: 'static, TWM: WMTrait> MtSticky<T, TWM> {
    /// Construct a `MtSticky` without thread checking.
    #[inline]
    pub const unsafe fn new_unchecked(x: T) -> Self {
        Self {
            _phantom: PhantomData,
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
    pub fn get_with_wm(&self, _: WM) -> &T {
        unsafe { &*self.get_ptr() }
    }

    /// Get a mutable reference to the inner value with compile-time thread checking.
    #[inline]
    pub fn get_mut_with_wm(&mut self, _: WM) -> &mut T {
        unsafe { &mut *self.get_ptr() }
    }
}

impl<T: 'static, TWM: WMTrait> Drop for MtSticky<T, TWM> {
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
pub struct MtLock<T, TWM: WMTrait = WM> {
    _phantom: PhantomData<TWM>,
    cell: UnsafeCell<T>,
}

unsafe impl<T: Send, TWM: WMTrait> Send for MtLock<T, TWM> {}
unsafe impl<T: Send, TWM: WMTrait> Sync for MtLock<T, TWM> {}

impl<T: fmt::Debug, TWM: WMTrait> fmt::Debug for MtLock<T, TWM> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(wm) = WM::try_global() {
            f.debug_tuple("MtLock").field(self.get_with_wm(wm)).finish()
        } else {
            write!(f, "MtLock(<not main thread>)")
        }
    }
}

#[allow(dead_code)]
impl<T, TWM: WMTrait> MtLock<T, TWM> {
    /// Construct a `MtLock`.
    #[inline]
    pub const fn new(x: T) -> Self {
        Self {
            _phantom: PhantomData,
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
    pub fn get_with_wm(&self, _: WM) -> &T {
        unsafe { &*self.get_ptr() }
    }
}

/// A trait implemented by variables generated by the [`mt_lazy_static`] macro.
pub trait MtLazyStatic {
    type Target;

    /// Initialize and get the inner value with compile-time thread checking.
    fn get_with_wm(&self, _: WM) -> &Self::Target;

    /// Initialize and get the inner value without thread checking.
    unsafe fn get_unchecked(&self) -> &Self::Target {
        self.get_with_wm(WM::global_unchecked())
    }
}

/// Like `lazy_static!`, but only accessible by the main thread. Can be used
/// for `!Send + !Sync` types. The defined variable implements [`MtLazyStatic`].
///
/// [`MtLazyStatic`]: crate::pal::cells::MtLazyStatic
///
/// # Examples
///
/// ```
/// use tcw3::{mt_lazy_static, pal::{prelude::*, LayerAttrs, HLayer}};
/// # fn hoge(wm: tcw3::pal::WM) {
/// mt_lazy_static! {
///     static ref LAYER: HLayer => |wm| wm.new_layer(LayerAttrs::default());
/// }
///
/// let layer = LAYER.get_with_wm(wm);
/// # }
/// ```
#[macro_export]
macro_rules! mt_lazy_static {
    (
        $vis:vis static ref $name:ident: $type:ty => $init:expr;
        $($rest:tt)*
    ) => {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        $vis struct $name {
            cell: ::std::cell::UnsafeCell<::std::option::Option<$type>>,
            initing: ::std::cell::Cell<bool>,
        }

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl $name {
            #[cold]
            fn __init_cell(wm: $crate::pal::WM) -> &'static $type {
                assert!(!$name.initing.get(), "recursion detected while lazily initializing a global variable");
                $name.initing.set(true);

                let initer: fn($crate::pal::WM) -> $type = $init;

                let value = initer(wm);

                unsafe {
                    $name.cell.get().write(Some(value));
                    (&*($name.cell.get() as *const ::std::option::Option<$type>)).as_ref().unwrap()
                }
            }
        }

        impl $crate::pal::prelude::MtLazyStatic for $name {
            type Target = $type;

            #[inline]
            fn get_with_wm(&self, wm: $crate::pal::WM) -> &$type {
                unsafe {
                    if let Some(inner) = (*self.cell.get()).as_ref() {
                        inner
                    } else {
                        Self::__init_cell(wm)
                    }
                }
            }
        }

        $vis static $name: $name = $name {
            cell: ::std::cell::UnsafeCell::new(None),
            initing: ::std::cell::Cell::new(false),
        };

        $crate::mt_lazy_static! { $($rest)* }
    };
    () => {};
}
