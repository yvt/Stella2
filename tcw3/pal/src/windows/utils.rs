use std::{cell::Cell, fmt, mem::MaybeUninit, ptr::NonNull};
use winapi::{
    shared::ntdef::HRESULT,
    um::{errhandlingapi::GetLastError, unknwnbase::IUnknown},
    Interface,
};

use super::{textinput::tsf, winapiext};

/// Check the given `HRESULT` and panic if it's not `S_OK`.
pub fn assert_hresult_ok(result: HRESULT) -> HRESULT {
    if result < 0 {
        panic_hresult(result);
    } else {
        result
    }
}

/// Discriminate `result` by whether it represents a successful code or not.
pub fn result_from_hresult(result: HRESULT) -> Result<HRESULT, HRESULT> {
    if result < 0 {
        Err(result)
    } else {
        Ok(result)
    }
}

pub fn hresult_from_result_with(func: impl FnOnce() -> Result<HRESULT, HRESULT>) -> HRESULT {
    let result = func();
    let flattened = result.unwrap_or_else(|x| x);

    // `Ok` and `Err` must represent success and failure respectively
    debug_assert_eq!(result, result_from_hresult(flattened));

    flattened
}

#[cold]
pub fn panic_hresult(result: HRESULT) -> ! {
    panic!("HRESULT = 0x{:08x}", result);
}

/// Panic with an error code returned by `GetLastError` if the
/// given value is equal to `T::default()` (e.g., `FALSE`, `false`).
pub fn assert_win32_ok<T: Default + PartialEq<T> + Copy>(b: T) {
    if b == T::default() {
        panic_last_error();
    }
}

/// Panic with an error code returned by `GetLastError` if the
/// given pointer is null.
pub fn assert_win32_nonnull<T: IsNull>(b: T) -> T {
    if b.is_null() {
        panic_last_error();
    }
    b
}

pub trait IsNull {
    fn is_null(&self) -> bool;
}

impl<T: ?Sized> IsNull for *const T {
    fn is_null(&self) -> bool {
        (*self).is_null()
    }
}
impl<T: ?Sized> IsNull for *mut T {
    fn is_null(&self) -> bool {
        (*self).is_null()
    }
}

/// Panic with an error code returned by `GetLastError`.
#[cold]
pub fn panic_last_error() -> ! {
    panic!("Win32 error 0x{:08x}", unsafe { GetLastError() });
}

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
    winapi::um::d2d1_1::ID2D1Device,
    winapi::um::d2d1_1::ID2D1DeviceContext,
    winapi::shared::dxgi::IDXGIDevice,
    winapiext::ID3D11Device4,
    winapiext::ICompositorDesktopInterop,
    winapiext::ICompositorInterop,
    winapiext::ICompositionGraphicsDeviceInterop,
    winapiext::ICompositionDrawingSurfaceInterop,
    tsf::ITfContext,
    tsf::ITfDocumentMgr,
    tsf::ITfThreadMgr,
    tsf::ITfKeystrokeMgr,
    tsf::ITfMessagePump,
    tsf::ITextStoreACPSink,
}

/// Smart pointer for COM objects.
pub struct ComPtr<T: Object>(NonNull<T>);

impl<T: Object> fmt::Debug for ComPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.0)
    }
}

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

#[allow(dead_code)]
impl<T: Object> ComPtr<T> {
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        Self(ptr)
    }

    pub unsafe fn from_ptr_unchecked(ptr: *mut T) -> Self {
        debug_assert!(!ptr.is_null());
        Self::new(NonNull::new_unchecked(ptr))
    }

    pub unsafe fn from_ptr(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|p| Self::new(p))
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
        unsafe { query_interface(self.as_iunknown().into()) }
    }

    pub fn into_winrt_comptr(self) -> winrt::ComPtr<T> {
        unsafe { winrt::ComPtr::wrap(self.into_raw().as_ptr()) }
    }
}

pub unsafe fn query_interface<S: Object>(iunk: NonNull<IUnknown>) -> Option<ComPtr<S>> {
    let mut out = MaybeUninit::uninit();
    result_from_hresult(iunk.as_ref().QueryInterface(&S::uuidof(), out.as_mut_ptr())).ok()?;
    let out = out.assume_init();

    debug_assert!(!out.is_null());
    Some(ComPtr::from_ptr_unchecked(out as _))
}

impl ComPtr<IUnknown> {
    pub fn iunknown_from_winrt_comptr<T>(from: winrt::ComPtr<T>) -> Self {
        let p = (&*from) as *const T;
        std::mem::forget(from);
        unsafe { Self::from_ptr_unchecked(p as _) }
    }
}

/// Extends `Option<ComPtr<T>>` with `as_ptr`.
pub trait ComPtrAsPtr {
    type Output;

    fn as_ptr(&self) -> *mut Self::Output;
}

impl<T: Object> ComPtrAsPtr for ComPtr<T> {
    type Output = T;

    fn as_ptr(&self) -> *mut Self::Output {
        self.as_ptr()
    }
}

impl<T: Object> ComPtrAsPtr for Option<ComPtr<T>> {
    type Output = T;

    fn as_ptr(&self) -> *mut Self::Output {
        if let Some(inner) = self {
            inner.as_ptr()
        } else {
            std::ptr::null_mut()
        }
    }
}

fn cell_map<T: Default, R>(cell: &Cell<T>, map: impl FnOnce(&mut T) -> R) -> R {
    let mut val = cell.take();
    let ret = map(&mut val);
    cell.set(val);
    ret
}

// TODO: This function was copied from `macos/window.rs`. De-duplicate
/// Clone the contents of `Cell<T>` by temporarily moving out the contents.
pub fn cell_get_by_clone<T: Clone + Default>(cell: &Cell<T>) -> T {
    cell_map(cell, |inner| inner.clone())
}
