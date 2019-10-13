//! Compositor for the testing backend.
use cggeom::{box2, prelude::*, Box2};
use iterpool::{Pool, PoolPtr};
use std::cell::RefCell;

use super::super::{
    iface, swrast,
    winit::{WinitWmCore, WndContent as WndContentTrait},
};
use super::{bitmap::Bitmap, Wm};

pub type WndAttrs<'a> = iface::WndAttrs<'a, Wm, HLayer>;
pub type LayerAttrs = iface::LayerAttrs<Bitmap, HLayer>;

pub(super) struct Screen {
    state: RefCell<State>,
}

#[derive(Debug, Clone)]
pub struct HWnd {
    /// A pointer into `State::wnds`.
    ptr: PoolPtr,
}

#[derive(Debug, Clone)]
pub struct HLayer {
    sr_layer: swrast::HLayer,
}

struct State {
    binner: swrast::Binner<Bitmap>,
    sr_scrn: swrast::Screen<Bitmap>,
    wnds: Pool<Wnd>,
}

pub struct Wnd {
    sr_wnd: swrast::HWnd,

    surf_size: [u32; 2],
    surf_dpi_scale: f32,

    dirty_rect: Option<Box2<usize>>,
}

impl Screen {
    pub(super) fn new(wm: Wm) -> Self {
        let state = State {
            binner: swrast::Binner::new(),
            sr_scrn: swrast::Screen::new(),
            wnds: Pool::new(),
        };

        Self {
            state: RefCell::new(state),
        }
    }

    pub(super) fn new_wnd(&self, attrs: WndAttrs<'_>) -> HWnd {
        let mut state = self.state.borrow_mut();

        let layer = attrs.layer.unwrap_or(None);

        let wnd = Wnd {
            sr_wnd: state.sr_scrn.new_wnd(),
            surf_size: [0, 0],
            surf_dpi_scale: 1.0,
            dirty_rect: None,
        };

        state
            .sr_scrn
            .set_wnd_layer(&wnd.sr_wnd, layer.map(|hl| hl.sr_layer));

        let ptr = state.wnds.allocate(wnd);
        HWnd { ptr }
    }

    pub(super) fn set_wnd_attr(&self, hwnd: &HWnd, attrs: WndAttrs<'_>) {
        unimplemented!()
    }
    pub(super) fn remove_wnd(&self, hwnd: &HWnd) {
        unimplemented!()
    }
    pub(super) fn update_wnd(&self, hwnd: &HWnd) {
        unimplemented!()
    }
    pub(super) fn get_wnd_size(&self, hwnd: &HWnd) -> [u32; 2] {
        unimplemented!()
    }
    pub(super) fn get_wnd_dpi_scale(&self, hwnd: &HWnd) -> f32 {
        unimplemented!()
    }

    pub(super) fn new_layer(&self, attrs: LayerAttrs) -> HLayer {
        let mut state = self.state.borrow_mut();

        HLayer {
            sr_layer: state
                .sr_scrn
                .new_layer(layer_attrs_to_sr_layer_attrs(attrs)),
        }
    }
    pub(super) fn set_layer_attr(&self, layer: &HLayer, attrs: LayerAttrs) {
        let mut state = self.state.borrow_mut();

        state
            .sr_scrn
            .set_layer_attr(&layer.sr_layer, layer_attrs_to_sr_layer_attrs(attrs));
    }
    pub(super) fn remove_layer(&self, layer: &HLayer) {
        let mut state = self.state.borrow_mut();

        state.sr_scrn.remove_layer(&layer.sr_layer);
    }
}

/// Convert the `LayerAttrs` of `Wm` to the `LayerAttrs` of `swrast`.
/// Copied straight from `unix/comp.rs`.
fn layer_attrs_to_sr_layer_attrs(attrs: LayerAttrs) -> iface::LayerAttrs<Bitmap, swrast::HLayer> {
    iface::LayerAttrs {
        transform: attrs.transform,
        contents: attrs.contents,
        bounds: attrs.bounds,
        contents_center: attrs.contents_center,
        contents_scale: attrs.contents_scale,
        bg_color: attrs.bg_color,
        sublayers: attrs.sublayers.map(|sublayers| {
            sublayers
                .into_iter()
                .map(|hlayer| hlayer.sr_layer)
                .collect()
        }),
        opacity: attrs.opacity,
        flags: attrs.flags,
    }
}
