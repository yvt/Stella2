use winit::window::Window;

use super::super::{
    winit::{HWndCore, WinitWmCore, WinitWm, WndContent as WndContentTrait},
    WndAttrs,
};
use super::{HLayer, Wm};

#[derive(Debug, Clone)]
pub struct HWnd {
    winit_hwnd: HWndCore,
}

pub(super) struct WndContent {}

impl WndContentTrait for WndContent {
    type Wm = Wm;
    type HLayer = HLayer;

    fn set_layer(
        &mut self,
        _wm: &WinitWmCore<Self::Wm, Self>,
        _winit_wnd: &Window,
        _layer: Option<Self::HLayer>,
    ) {
        // TODO
    }
}

impl WinitWm for Wm {
    fn hwnd_core_to_hwnd(self, hwnd: &HWndCore) -> Self::HWnd {
        HWnd {
            winit_hwnd: hwnd.clone(),
        }
    }
}

impl HWnd {
    pub(super) fn new(wm: Wm, attrs: WndAttrs<'_>) -> Self {
        let winit_hwnd = wm.winit_wm().new_wnd(attrs, |_winit_window, _layer| {
            // TODO
            WndContent {}
        });

        Self { winit_hwnd }
    }

    pub(super) fn set_attrs(&self, wm: Wm, attrs: WndAttrs<'_>) {
        wm.winit_wm().set_wnd_attr(&self.winit_hwnd, attrs);
    }

    pub(super) fn remove(&self, wm: Wm) {
        wm.winit_wm().remove_wnd(&self.winit_hwnd);
    }

    pub(super) fn update(&self, wm: Wm) {
        wm.winit_wm().update_wnd(&self.winit_hwnd)
    }

    pub(super) fn get_size(&self, wm: Wm) -> [u32; 2] {
        wm.winit_wm().get_wnd_size(&self.winit_hwnd)
    }

    pub(super) fn get_dpi_scale(&self, wm: Wm) -> f32 {
        wm.winit_wm().get_wnd_dpi_scale(&self.winit_hwnd)
    }
}
