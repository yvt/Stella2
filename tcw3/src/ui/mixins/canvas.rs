use cggeom::{prelude::*, Box2};
use cgmath::{Matrix3, Vector2};

use crate::{
    pal,
    pal::prelude::*,
    uicore::{HView, HWnd, Sub, UpdateCtx},
};

/// A view listener mix-in that allows the client to add `Canvas`-based 2D
/// drawing to a `ViewListener`.
#[derive(Debug)]
pub struct CanvasMixin {
    state: Option<MountState>,
}

#[derive(Debug)]
struct MountState {
    layer: pal::HLayer,
    sub: Sub,
    last_size: Option<[u32; 2]>,
}

#[derive(Debug)]
pub struct DrawContext<'a> {
    /// A `BitmapBuilder` object implementing [`Canvas`], with which the client
    /// should draw the layer contents to a backing store.
    ///
    /// [`Canvas`]: crate::pal::iface::Canvas
    pub canvas: &'a mut pal::BitmapBuilder,

    /// The size of the backing store measured in points (virtual pixels).
    pub size: Vector2<f32>,

    /// The DPI scaling ratio.
    ///
    /// `canvas` is already scaled by this value.
    pub dpi_scale: f32,
}

impl CanvasMixin {
    /// Construct a `CanvasMixin`.
    pub fn new() -> Self {
        Self { state: None }
    }

    /// Implements [`ViewListener::mount`].
    ///
    /// [`ViewListener::mount`]: crate::uicore::ViewListener::mount
    pub fn mount(&mut self, wm: pal::WM, view: &HView, wnd: &HWnd) {
        assert!(self.state.is_none());

        let layer = wm.new_layer(&pal::LayerAttrs {
            ..Default::default()
        });

        let sub = {
            let view = view.clone();
            wnd.subscribe_dpi_scale_changed(Box::new(move |_, _| {
                view.pend_update();
            }))
        };

        self.state = Some(MountState {
            layer,
            sub,
            last_size: None,
        });
    }

    /// Implements [`ViewListener::unmount`].
    ///
    /// [`ViewListener::unmount`]: crate::uicore::ViewListener::unmount
    pub fn unmount(&mut self, wm: pal::WM, _: &HView) {
        let state = self.state.take().expect("not mounted");
        wm.remove_layer(&state.layer);
        state.sub.unsubscribe().unwrap();
    }

    /// Implements [`ViewListener::position`].
    ///
    /// [`ViewListener::position`]: crate::uicore::ViewListener::position
    pub fn position(&mut self, _: pal::WM, view: &HView) {
        assert!(self.state.is_some(), "not mounted");
        view.pend_update();
    }

    /// Get the backing layer if mounted.
    pub fn layer(&self) -> Option<&pal::HLayer> {
        self.state.as_ref().map(|s| &s.layer)
    }

    /// Update the backing layer. The caller-supplied draw function is used
    /// to provide new layer contents if necessary.
    pub fn update_layer(
        &mut self,
        wm: pal::WM,
        view: &HView,
        wnd: &HWnd,
        draw: impl FnOnce(&mut DrawContext<'_>),
    ) {
        let state = self.state.as_mut().expect("not mounted");

        let layer = &state.layer;

        let view_frame = view.global_frame();
        let view_size = view_frame.size();
        let dpi_scale = wnd.dpi_scale();

        // Calculate the new bitmap size
        let bmp_size = [
            (view_size.x * dpi_scale).max(1.0).ceil() as u32,
            (view_size.y * dpi_scale).max(1.0).ceil() as u32,
        ];
        let bmp_pt_size = Vector2::from(bmp_size).cast::<f32>().unwrap() / dpi_scale;

        // (Re-)create the bitmap if needed
        let bmp = if Some(bmp_size) != state.last_size {
            let mut builder = pal::BitmapBuilder::new(bmp_size);

            // Apply DPI scaling
            builder.mult_transform(Matrix3::from_scale_2d(dpi_scale));

            // Call the draw function
            draw(&mut DrawContext {
                canvas: &mut builder,
                size: bmp_pt_size,
                dpi_scale,
            });

            state.last_size = Some(bmp_size);

            Some(builder.into_bitmap())
        } else {
            None
        };

        // Calculate the new layer bounds
        let bounds = Box2::new(view_frame.min, view_frame.min + bmp_pt_size);

        wm.set_layer_attr(
            layer,
            &pal::LayerAttrs {
                contents: bmp.map(Some),
                bounds: Some(bounds),
                ..Default::default()
            },
        );
    }

    /// Implements [`ViewListener::update`] using a caller-supplied draw
    /// function.
    ///
    /// [`ViewListener::update`]: crate::uicore::ViewListener::update
    ///
    /// This method internally calls `UpdateCtx::set_layers`. If you need
    /// more control over a view's backing layers, you should use
    /// [`Self::update_layer`] and [`Self::layer`] instead.
    pub fn update(
        &mut self,
        wm: pal::WM,
        view: &HView,
        ctx: &mut UpdateCtx<'_>,
        draw: impl FnOnce(&mut DrawContext<'_>),
    ) {
        self.update_layer(wm, view, ctx.hwnd(), draw);

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![self.layer().unwrap().clone()]);
        }
    }

    /// Pend a redraw.
    ///
    /// This method updates an internal flag and calls [`View::pend_redraw`].
    /// As a result, a caller-supplied draw function will be used to update
    /// the layer contents when `update` is called for the next time.
    ///
    /// [`View::pend_redraw`]: crate::uicore::View::pend_redraw
    pub fn pend_draw(&mut self, view: &HView) {
        if let Some(state) = &mut self.state {
            state.last_size = None;
            view.pend_update();
        }
    }
}
