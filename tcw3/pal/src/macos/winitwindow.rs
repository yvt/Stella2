use super::super::{
    WndAttrs, winit::{WndContent as WndContentTrait},
};
use super::WM;

#[derive(Debug, Clone)]
pub struct HWnd {
}

pub(super) struct WndContent {}

impl WndContentTrait for WndContent {}

impl HWnd {
    /// Must be called from a main thread.
    pub(super) unsafe fn new(_attrs: WndAttrs<'_>) -> Self {
        unimplemented!()
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn set_attrs(&self, _attrs: WndAttrs<'_>) {
        unimplemented!()
    }

    /// Must be called from a main thread.
    pub(super) unsafe fn remove(&self) {
        unimplemented!()
    }

    pub(super) fn update(&self, _: WM) {
        unimplemented!()
    }

    pub(super) fn get_size(&self, _: WM) -> [u32; 2] {
        unimplemented!()
    }

    pub(super) fn get_dpi_scale(&self, _: WM) -> f32 {
        unimplemented!()
    }
}
