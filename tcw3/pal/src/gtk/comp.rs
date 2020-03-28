//! Compositor.
//!
//! This will probably be superseded by a GSK backend when GTK 4 is released,
//! or by a Cairo backend if the adoption of GTK 4 is not fast enough.
use cairo::ImageSurface;
use cggeom::{box2, prelude::*, Box2};

use super::{Bitmap, LayerAttrs};
use crate::{iface, swrast};

/// The global state of the compositor.
///
/// This stores references to objects possibly shared by multiple windows, such
/// as off-screen image buffers.
pub(super) struct Compositor {
    binner: swrast::Binner<Bitmap>,
    sr_scrn: swrast::Screen<Bitmap>,
}

pub struct Wnd {
    cairo_img: Option<ImageSurface>,
    sr_wnd: swrast::HWnd<Bitmap>,

    surf_size: [usize; 2],
    surf_dpi_scale: f32,

    dirty_rect: Option<Box2<usize>>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct HLayer {
    sr_layer: swrast::HLayer<Bitmap>,
}

impl Compositor {
    pub(super) const fn new() -> Self {
        Self {
            binner: swrast::Binner::new(),
            sr_scrn: swrast::Screen::new(),
        }
    }

    pub(super) fn new_wnd(&mut self, layer: Option<HLayer>) -> Wnd {
        let wnd = Wnd {
            cairo_img: None,
            sr_wnd: self.sr_scrn.new_wnd(),
            surf_size: [0, 0],
            surf_dpi_scale: 1.0,
            dirty_rect: None,
        };

        self.sr_scrn
            .set_wnd_layer(&wnd.sr_wnd, layer.map(|hl| hl.sr_layer));

        wnd
    }

    pub(super) fn remove_wnd(&mut self, wnd: &Wnd) {
        self.sr_scrn.remove_wnd(&wnd.sr_wnd);
    }

    pub(super) fn new_layer(&mut self, attrs: LayerAttrs) -> HLayer {
        HLayer {
            sr_layer: self.sr_scrn.new_layer(layer_attrs_to_sr_layer_attrs(attrs)),
        }
    }

    pub(super) fn set_layer_attr(&mut self, layer: &HLayer, attrs: LayerAttrs) {
        self.sr_scrn
            .set_layer_attr(&layer.sr_layer, layer_attrs_to_sr_layer_attrs(attrs));
    }

    pub(super) fn remove_layer(&mut self, layer: &HLayer) {
        self.sr_scrn.remove_layer(&layer.sr_layer);
    }

    pub(super) fn set_wnd_layer(&mut self, wnd: &Wnd, layer: Option<HLayer>) {
        self.sr_scrn
            .set_wnd_layer(&wnd.sr_wnd, layer.map(|hl| hl.sr_layer));
    }

    /// Analyze updates in the layer tree and return a newly-added dirty
    /// rectangle. At the same time, resizes the backing store to match the
    /// specified size.
    ///
    /// `surf_size_sz` and `surf_dpi_scale` specify the desired properties of
    /// the backing store.
    pub(super) fn update_wnd(
        &mut self,
        wnd: &mut Wnd,
        surf_size_sz: [usize; 2],
        surf_dpi_scale: f32,
    ) -> Option<Box2<usize>> {
        // Check the surface size
        let [size_w, size_h] = surf_size_sz;
        if size_w == 0 || size_h == 0 {
            return None;
        }

        let should_renew_surface =
            (surf_size_sz, surf_dpi_scale) != (wnd.surf_size, wnd.surf_dpi_scale);

        if should_renew_surface {
            // Update the surface size
            let cairo_img = ImageSurface::create(
                cairo::Format::ARgb32,
                surf_size_sz[0] as i32,
                surf_size_sz[1] as i32,
            )
            .unwrap();
            cairo_img.set_device_scale(surf_dpi_scale as f64, surf_dpi_scale as f64);
            wnd.cairo_img = Some(cairo_img);

            self.sr_scrn.set_wnd_size(&wnd.sr_wnd, surf_size_sz);
            self.sr_scrn.set_wnd_dpi_scale(&wnd.sr_wnd, surf_dpi_scale);

            wnd.surf_size = surf_size_sz;
            wnd.surf_dpi_scale = surf_dpi_scale;
            wnd.dirty_rect = Some(box2! { min: [0, 0].into(), max: surf_size_sz.into() });
        }

        // Compute the dirty region
        let new_dirty = self.sr_scrn.update_wnd(&wnd.sr_wnd);

        if let Some(new_dirty) = new_dirty {
            if let Some(x) = &mut wnd.dirty_rect {
                x.union_assign(&new_dirty);
            } else {
                wnd.dirty_rect = Some(new_dirty);
            }
        }

        new_dirty
    }

    /// Render the contents of `Wnd::cairo_surface()`. Returns a rectangle
    /// encompassing the re-rendered rectangle.
    pub(super) fn paint_wnd(&mut self, wnd: &mut Wnd) -> Option<Box2<usize>> {
        if let Some(dirty_rect) = wnd.dirty_rect.take() {
            // Paint the image
            let image = wnd.cairo_img.as_mut().unwrap();
            let image_stride = image.get_stride() as usize;
            let mut image_data = image.get_data().unwrap();
            self.sr_scrn.render_wnd(
                &wnd.sr_wnd,
                &mut image_data[dirty_rect.min.x * 4 + dirty_rect.min.y * image_stride..],
                image_stride,
                dirty_rect,
                &mut self.binner,
            );
            Some(dirty_rect)
        } else {
            None
        }
    }
}

/// Convert the `LayerAttrs` of `Wm` to the `LayerAttrs` of `swrast`.
fn layer_attrs_to_sr_layer_attrs(
    attrs: LayerAttrs,
) -> iface::LayerAttrs<Bitmap, swrast::HLayer<Bitmap>> {
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

impl Wnd {
    /// Get the `ImageSurface` holding the latest rendered image of the window
    /// contents.
    pub(super) fn cairo_surface(&self) -> Option<&ImageSurface> {
        self.cairo_img.as_ref()
    }
}
