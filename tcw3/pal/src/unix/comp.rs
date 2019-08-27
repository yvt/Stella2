//! Compositor
use winit::window::Window;

use super::super::winit::{WinitWmCore, WndContent as WndContentTrait};
use super::{LayerAttrs, Wm};

/// The global state of the compositor.
///
/// This stores references to objects possibly shared by multiple windows, such
/// as a Wayland connection and Vulkan devices.
pub(super) struct Compositor {
    // TODO
}

pub struct WndContent {
    // TODO
}

#[derive(Debug, Clone)]
pub struct HLayer {
    // TODO
}

impl Compositor {
    pub(super) fn new(wm: Wm) -> Self {
        // `Compositor` is to be created before entering the main event loop, so
        // the following `unwrap` should succeed
        let event_loop = wm.winit_wm_core().event_loop().unwrap();

        Self {}
    }

    pub(super) fn new_wnd(&self, winit_wnd: &Window, layer: Option<HLayer>) -> WndContent {
        // TODO
        WndContent {}
    }

    pub(super) fn new_layer(&self, attrs: LayerAttrs) -> HLayer {
        HLayer {}
        // TODO
    }
    pub(super) fn set_layer_attr(&self, layer: &HLayer, attrs: LayerAttrs) {
        // TODO
    }
    pub(super) fn remove_layer(&self, layer: &HLayer) {
        // TODO
    }
}

impl WndContentTrait for WndContent {
    type Wm = Wm;
    type HLayer = HLayer;

    fn set_layer(
        &mut self,
        wm: &WinitWmCore<Self::Wm, Self>,
        winit_wnd: &Window,
        layer: Option<Self::HLayer>,
    ) {
        // TODO
    }
}
