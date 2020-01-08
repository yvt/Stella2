//! Compositor
use winapi::shared::ntdef::HRESULT;
use winrt::{windows::ui::composition::Compositor, ComPtr, RtDefaultConstructible};

use super::{surface, LayerAttrs, Wm};
use crate::prelude::MtLazyStatic;

struct CompState {
    compositor: ComPtr<Compositor>,
    surface_map: surface::SurfaceMap,
}

impl CompState {
    fn new(_: Wm) -> Self {
        // Create a dispatch queue for the main thread
        unsafe {
            assert_eq!(tcw_comp_init(), 0);
        }

        let compositor = Compositor::new();

        let surface_map = surface::SurfaceMap::new(&compositor);

        CompState {
            compositor,
            surface_map,
        }
    }
}

mt_lazy_static! {
    static <Wm> ref CS: CompState => CompState::new;
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
    let _ = CS.get_with_wm(wm);

    log::warn!("new_layer: stub!");
    HLayer {}
}
pub fn set_layer_attr(_: Wm, layer: &HLayer, attrs: LayerAttrs) {
    log::warn!("set_layer_attr: stub!");
}
pub fn remove_layer(_: Wm, layer: &HLayer) {
    log::warn!("remove_layer: stub!");
}
