//! Compositor
use winapi::shared::ntdef::HRESULT;
use winrt::{windows::ui::composition::Compositor, ComPtr, RtDefaultConstructible};

use super::{LayerAttrs, Wm};
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HLayer {
    // TODO
}

pub fn new_layer(wm: Wm, attrs: LayerAttrs) -> HLayer {
    let _ = COMPOSITOR.get_with_wm(wm);

    log::warn!("new_layer: stub!");
    HLayer {}
}
pub fn set_layer_attr(_: Wm, layer: &HLayer, attrs: LayerAttrs) {
    log::warn!("set_layer_attr: stub!");
}
pub fn remove_layer(_: Wm, layer: &HLayer) {
    log::warn!("remove_layer: stub!");
}
