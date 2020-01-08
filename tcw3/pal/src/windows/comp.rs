//! Compositor
use winapi::shared::ntdef::HRESULT;
use winrt::{windows::ui::composition::Compositor, ComPtr, RtDefaultConstructible};

use super::Wm;
use crate::prelude::MtLazyStatic;

mt_lazy_static! {
    static <Wm> ref COMPOSITOR: ComPtr<Compositor> =>
        |_| {
            // Create a dispatch queue for the main thread
            unsafe {
                assert_eq!(tcw_comp_init(), 0);
            }

            Compositor::new()
        };
}

// Defined in `comp.cpp`
extern "C" {
    fn tcw_comp_init() -> HRESULT;
}
