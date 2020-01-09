//! Compositor
use std::{fmt, mem::MaybeUninit};
use winapi::shared::{ntdef::HRESULT, windef::HWND};
use winrt::{
    windows::ui::composition::{desktop::IDesktopWindowTarget, Compositor, ICompositionTarget},
    ComPtr, RtDefaultConstructible, RtType,
};

use super::{
    surface,
    utils::{assert_hresult_ok, ComPtr as MyComPtr},
    winapiext::ICompositorDesktopInterop,
    LayerAttrs, Wm,
};
use crate::prelude::MtLazyStatic;

struct CompState {
    comp: ComPtr<Compositor>,
    comp_desktop: MyComPtr<ICompositorDesktopInterop>,
    surface_map: surface::SurfaceMap,
}

impl CompState {
    fn new(_: Wm) -> Self {
        // Create a dispatch queue for the main thread
        unsafe {
            assert_hresult_ok(tcw_comp_init());
        }

        let comp = Compositor::new();

        let comp_desktop: MyComPtr<ICompositorDesktopInterop> =
            MyComPtr::iunknown_from_winrt_comptr(comp.clone())
                .query_interface()
                .unwrap();

        let surface_map = surface::SurfaceMap::new(&comp);

        CompState {
            comp,
            comp_desktop,
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

pub(super) struct CompWnd {
    target: ComPtr<ICompositionTarget>,
}

impl fmt::Debug for CompWnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompWnd")
            .field("target", &(&*self.target as *const _))
            .finish()
    }
}

impl CompWnd {
    pub(super) fn new(wm: Wm, hwnd: HWND) -> Self {
        let cs = CS.get_with_wm(wm);

        let desktop_target = unsafe {
            let mut out = MaybeUninit::uninit();
            assert_hresult_ok(
                cs.comp_desktop
                    .CreateDesktopWindowTarget(hwnd, 0, out.as_mut_ptr()),
            );
            IDesktopWindowTarget::wrap(out.assume_init()).unwrap()
        };

        let target = desktop_target.query_interface().unwrap();

        Self { target }
    }

    pub(super) fn set_layer(&self, layer: Option<HLayer>) {
        log::warn!("set_layer: stub!");
    }
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
