//! Compositor for the testing backend.
use cggeom::{box2, Box2};
use iterpool::{Pool, PoolPtr};
use std::cell::RefCell;

use super::super::{iface, swrast};
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

    size: [u32; 2],
    dpi_scale: f32,

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
            size: attrs.size.unwrap_or([100, 100]),
            dpi_scale: 1.0, // TODO
            dirty_rect: None,
        };

        // TODO

        state
            .sr_scrn
            .set_wnd_layer(&wnd.sr_wnd, layer.map(|hl| hl.sr_layer));

        let ptr = state.wnds.allocate(wnd);
        HWnd { ptr }
    }

    pub(super) fn set_wnd_attr(&self, hwnd: &HWnd, attrs: WndAttrs<'_>) {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // enable split borrow

        let wnd = &mut state.wnds[hwnd.ptr];

        if let Some(value) = attrs.size {
            wnd.size = value;
        }

        // TODO:
        // - `min_size`
        // - `max_size`
        // - `flags`
        // - `caption`
        // - `visible`
        // - `listener`

        if let Some(layer) = attrs.layer {
            state
                .sr_scrn
                .set_wnd_layer(&wnd.sr_wnd, layer.map(|hl| hl.sr_layer));
        }
    }
    pub(super) fn remove_wnd(&self, hwnd: &HWnd) {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // enable split borrow

        let wnd = state.wnds.deallocate(hwnd.ptr).expect("invalid hwnd");

        state.sr_scrn.remove_wnd(&wnd.sr_wnd);
    }
    pub(super) fn update_wnd(&self, hwnd: &HWnd) {
        let mut state = self.state.borrow_mut();
        let state = &mut *state; // enable split borrow
        let wnd = &mut state.wnds[hwnd.ptr];

        // Calculate the surface size
        let [size_w, size_h] = wnd.size;
        let dpi_scale = wnd.dpi_scale;
        let surf_size = [
            (size_w as f32 * dpi_scale) as usize,
            (size_h as f32 * dpi_scale) as usize,
        ];
        if surf_size[0] == 0 || surf_size[1] == 0 {
            // Suspend update if one of the surface dimensions is zero
            return;
        }

        // TODO: Preserve surface image
        let img_stride = 4usize.checked_mul(surf_size[0]).unwrap();
        let num_bytes = img_stride.checked_mul(surf_size[1]).unwrap();
        let mut img = vec![0u8; num_bytes];

        wnd.dirty_rect = Some(box2! { min: [0, 0], max: surf_size });
        state.sr_scrn.set_wnd_size(&wnd.sr_wnd, surf_size);
        state.sr_scrn.set_wnd_dpi_scale(&wnd.sr_wnd, wnd.dpi_scale);

        let dirty_rect = if let Some(x) = wnd.dirty_rect.take() {
            x
        } else {
            return;
        };

        state.sr_scrn.render_wnd(
            &wnd.sr_wnd,
            &mut img,
            img_stride,
            dirty_rect,
            &mut state.binner,
        );

        // TODO: Let the clients observe the rendered image
    }
    pub(super) fn get_wnd_size(&self, hwnd: &HWnd) -> [u32; 2] {
        let state = self.state.borrow();
        state.wnds[hwnd.ptr].size
    }
    pub(super) fn get_wnd_dpi_scale(&self, hwnd: &HWnd) -> f32 {
        let state = self.state.borrow();
        state.wnds[hwnd.ptr].dpi_scale
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
