//! Compositor
use cggeom::{box2, prelude::*, Box2};
use std::cell::RefCell;
use swsurface::{Context as SwContext, Surface as SwSurface};
use winit::window::Window;

use super::super::{
    iface, swrast,
    winit::{WinitWmCore, WndContent as WndContentTrait},
};
use super::{Bitmap, LayerAttrs, Wm};

/// The global state of the compositor.
///
/// This stores references to objects possibly shared by multiple windows, such
/// as a Wayland connection and Vulkan devices.
pub(super) struct Compositor {
    wm: Wm,
    sw_ctx: SwContext,
    state: RefCell<State>,
}

struct State {
    binner: swrast::Binner<Bitmap>,
    sr_scrn: swrast::Screen<Bitmap>,
    // TODO: GPU composition
}

pub struct WndContent {
    sw_surf: Option<SwSurface>,
    sr_wnd: swrast::HWnd,

    surf_size: [u32; 2],
    surf_dpi_scale: f32,

    /// The dirty region for each swapchain image.
    dirty_rect: Vec<Option<Box2<usize>>>,
}

#[derive(Debug, Clone)]
pub struct HLayer {
    sr_layer: swrast::HLayer,
}

impl Compositor {
    pub(super) fn new(wm: Wm) -> Self {
        // `Compositor` is to be created before entering the main event loop, so
        // the following `unwrap` should succeed
        let event_loop = wm.winit_wm_core().event_loop().unwrap();

        let state = State {
            binner: swrast::Binner::new(),
            sr_scrn: swrast::Screen::new(),
        };

        Self {
            wm,
            sw_ctx: swsurface::ContextBuilder::new(&event_loop)
                .with_ready_cb(|winit_wnd_id| {
                    // TODO
                })
                .build(),
            state: RefCell::new(state),
        }
    }

    pub(super) fn new_wnd(&self, winit_wnd: &Window, layer: Option<HLayer>) -> WndContent {
        let mut state = self.state.borrow_mut();

        // Unsafety: `SwSurface` must be dropped before the originating `Window`.
        //           See `<WndContext as WndContextTrait>::close`.
        let sw_surf = unsafe {
            SwSurface::new(
                winit_wnd,
                &self.sw_ctx,
                &swsurface::Config {
                    ..Default::default()
                },
            )
        };

        let content = WndContent {
            sw_surf: Some(sw_surf),
            sr_wnd: state.sr_scrn.new_wnd(),
            surf_size: [0, 0],
            surf_dpi_scale: 1.0,
            dirty_rect: Vec::new(),
        };

        state
            .sr_scrn
            .set_wnd_layer(&content.sr_wnd, layer.map(|hl| hl.sr_layer));

        content
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

impl WndContentTrait for WndContent {
    type Wm = Wm;
    type HLayer = HLayer;

    fn set_layer(
        &mut self,
        winit_wm_core: &WinitWmCore<Self::Wm, Self>,
        _winit_wnd: &Window,
        layer: Option<Self::HLayer>,
    ) {
        let mut state = winit_wm_core.wm().comp().state.borrow_mut();

        state
            .sr_scrn
            .set_wnd_layer(&self.sr_wnd, layer.map(|hl| hl.sr_layer));
    }

    fn update(&mut self, _wm: &WinitWmCore<Self::Wm, Self>, winit_wnd: &Window) {
        winit_wnd.request_redraw();
    }

    fn redraw_requested(
        &mut self,
        winit_wm_core: &WinitWmCore<Self::Wm, Self>,
        winit_wnd: &Window,
    ) {
        let mut state = winit_wm_core.wm().comp().state.borrow_mut();
        let state = &mut *state; // enable split borrow

        let sw_surf = self.sw_surf.as_mut().unwrap();

        // Check the surface size
        let (size_w, size_h): (u32, u32) = winit_wnd
            .inner_size()
            .to_physical(winit_wnd.hidpi_factor())
            .into();
        let surf_size = [size_w, size_h];
        let surf_size_sz: [usize; 2] = [size_w as _, size_h as _];
        let surf_dpi_scale = winit_wnd.hidpi_factor() as f32;

        if (surf_size, surf_dpi_scale) != (self.surf_size, self.surf_dpi_scale) {
            // Update the surface size
            sw_surf.update_surface(surf_size, swsurface::Format::Argb8888);

            state.sr_scrn.set_wnd_size(&self.sr_wnd, surf_size_sz);
            state
                .sr_scrn
                .set_wnd_dpi_scale(&self.sr_wnd, surf_dpi_scale);

            self.dirty_rect =
                vec![Some(box2! { min: [0, 0], max: surf_size_sz }); sw_surf.num_images()];
        }

        // Compute the dirty region
        if let Some(new_dirty) = state.sr_scrn.update_wnd(&self.sr_wnd) {
            for dirty_rect in self.dirty_rect.iter_mut() {
                if let Some(x) = dirty_rect {
                    x.union_assign(&new_dirty);
                } else {
                    *dirty_rect = Some(new_dirty);
                }
            }
        }

        if let Some(image_i) = sw_surf.poll_next_image() {
            let dirty_rect = if sw_surf.does_preserve_image() {
                // If the backend preserves swapchain images, we just have to
                // repaint the invalidated region
                if let Some(x) = self.dirty_rect[image_i].take() {
                    x
                } else {
                    return;
                }
            } else {
                // If the backend doesn't preserve swapchain images, we must
                // update entire the image for every frame
                box2! { min: [0, 0], max: surf_size_sz }
            };

            // Paint the swapchain image
            let mut image = sw_surf.lock_image(image_i);
            let image_info = sw_surf.image_info();
            let image_stride = image_info.stride;
            state.sr_scrn.render_wnd(
                &self.sr_wnd,
                &mut image[dirty_rect.min.x * 4 + dirty_rect.min.y * image_stride..],
                image_stride,
                dirty_rect,
                &mut state.binner,
            );
            drop(image);

            // Present the swapchain image
            sw_surf.present_image(image_i);
        } else {
            // In this case, `ready_cb` will called when the next image is ready
        }
    }

    fn close(&mut self, winit_wm_core: &WinitWmCore<Self::Wm, Self>, _winit_wnd: &Window) {
        // `swsurface::Surface` must be dropped before `Window`
        self.sw_surf = None;

        let mut state = winit_wm_core.wm().comp().state.borrow_mut();
        state.sr_scrn.remove_wnd(&self.sr_wnd);
    }
}

impl swrast::Bmp for Bitmap {
    fn data(&self) -> &[u8] {
        unimplemented!()
    }

    fn size(&self) -> [usize; 2] {
        unimplemented!()
    }

    fn stride(&self) -> usize {
        unimplemented!()
    }
}
