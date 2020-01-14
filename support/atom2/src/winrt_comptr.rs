use super::*;
use winrt::ComPtr;

unsafe impl<T> PtrSized for ComPtr<T> {
    fn into_raw(this: Self) -> NonNull<()> {
        let ptr = NonNull::from(&*this);
        std::mem::forget(this);
        ptr.cast()
    }
    unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        Self::wrap(ptr.as_ptr() as _)
    }
}
unsafe impl<T> TypedPtrSized for ComPtr<T> {
    type Target = T;
}
unsafe impl<T> MutPtrSized for ComPtr<T> {}
unsafe impl<T> TrivialPtrSized for ComPtr<T> {}
