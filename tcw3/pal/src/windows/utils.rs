use std::{mem::MaybeUninit, ptr::NonNull};
use winapi::{um::unknwnbase::IUnknown, Interface};

use super::winapiext;

/// Trait for interface types that inherit from `IUnknown`.
pub unsafe trait Object: Interface {
    fn as_iunknown(&self) -> &IUnknown;
}

macro_rules! unsafe_impl_object {
	() => {};
	($iface:ty $(, $(,)* $($rest:tt)* )?) => {
		unsafe impl Object for $iface {
			#[inline]
			fn as_iunknown(&self) -> &IUnknown {
				self
			}
		}
		unsafe_impl_object! { $($($rest)*)? }
	};
}

unsafe_impl_object! {
    winapi::um::unknwnbase::IUnknown,
    winapi::um::d3d11::ID3D11Device,
    winapiext::ID3D11Device4,
    winapiext::ICompositorInterop,
    winapiext::ICompositionGraphicsDeviceInterop,
    winapiext::ICompositionDrawingSurfaceInterop,
}

/// Smart pointer for COM objects.
#[derive(Debug)]
pub struct ComPtr<T: Object>(NonNull<T>);

impl<T: Object> Drop for ComPtr<T> {
    fn drop(&mut self) {
        unsafe {
            self.as_iunknown().Release();
        }
    }
}

impl<T: Object> Clone for ComPtr<T> {
    fn clone(&self) -> Self {
        unsafe {
            self.as_iunknown().AddRef();
        }
        Self(self.0)
    }
}

impl<T: Object> std::ops::Deref for ComPtr<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.as_ptr() }
    }
}

impl<T: Object> ComPtr<T> {
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        Self(ptr)
    }

    pub unsafe fn from_ptr_unchecked(ptr: *mut T) -> Self {
        debug_assert!(!ptr.is_null());
        Self::new(NonNull::new_unchecked(ptr))
    }

    pub fn into_raw(self) -> NonNull<T> {
        let p = self.as_non_null();
        std::mem::forget(self);
        p
    }

    pub fn as_non_null(&self) -> NonNull<T> {
        self.0
    }

    pub fn as_ptr(&self) -> *mut T {
        self.0.as_ptr()
    }

    pub fn query_interface<S: Object>(&self) -> Option<ComPtr<S>> {
        let mut out = MaybeUninit::uninit();
        let result = unsafe {
            self.as_iunknown()
                .QueryInterface(&S::uuidof(), out.as_mut_ptr())
        };
        if result == 0 {
            let out = unsafe { out.assume_init() };
            debug_assert!(!out.is_null());
            Some(unsafe { ComPtr::from_ptr_unchecked(out as _) })
        } else {
            None
        }
    }

    pub fn into_winrt_comptr(self) -> winrt::ComPtr<T> {
        unsafe { winrt::ComPtr::wrap(self.into_raw().as_ptr()) }
    }
}

impl ComPtr<IUnknown> {
    pub fn iunknown_from_winrt_comptr<T>(from: winrt::ComPtr<T>) -> Self {
        let p = (&*from) as *const T;
        std::mem::forget(from);
        unsafe { Self::from_ptr_unchecked(p as _) }
    }
}
