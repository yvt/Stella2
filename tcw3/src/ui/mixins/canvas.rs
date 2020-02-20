use cggeom::{box2, prelude::*, Box2};
use cgmath::{vec2, Matrix3, Point2, Vector2};
use std::cmp::max;

use crate::{
    pal,
    pal::prelude::*,
    uicore::{HView, HWndRef, Sub, UpdateCtx},
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
    last_phys_vis_bounds: Option<[Point2<i32>; 2]>,
}

#[derive(Debug)]
pub struct PaintContext<'a> {
    /// A `BitmapBuilder` object implementing [`Canvas`], with which the client
    /// should paint the layer contents to a backing store.
    ///
    /// [`Canvas`]: crate::pal::iface::Canvas
    ///
    /// When a paint function is called, `canvas` is configured to use the
    /// target view's coordinate space. This means that `(0, 0)` always matches
    /// the top-left corner of the view's frame and the coordinates are
    /// represented by logical pixels and are independent of physical pixel
    /// density.
    pub canvas: &'a mut pal::BitmapBuilder,

    /// The size of the backing store measured in points (virtual pixels).
    pub size: Vector2<f32>,

    /// The DPI scaling ratio.
    ///
    /// `canvas` is already scaled by this value.
    pub dpi_scale: f32,
}

impl Default for CanvasMixin {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasMixin {
    /// Construct a `CanvasMixin`.
    pub fn new() -> Self {
        Self { state: None }
    }

    /// Implements [`ViewListener::mount`].
    ///
    /// [`ViewListener::mount`]: crate::uicore::ViewListener::mount
    pub fn mount(&mut self, wm: pal::Wm, view: &HView, wnd: HWndRef<'_>) {
        assert!(self.state.is_none());

        let layer = wm.new_layer(pal::LayerAttrs {
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
            last_phys_vis_bounds: None,
        });

        view.pend_update();
    }

    /// Implements [`ViewListener::unmount`].
    ///
    /// [`ViewListener::unmount`]: crate::uicore::ViewListener::unmount
    pub fn unmount(&mut self, wm: pal::Wm, _: &HView) {
        let state = self.state.take().expect("not mounted");
        wm.remove_layer(&state.layer);
        state.sub.unsubscribe().unwrap();
    }

    /// Implements [`ViewListener::position`].
    ///
    /// [`ViewListener::position`]: crate::uicore::ViewListener::position
    pub fn position(&mut self, _: pal::Wm, view: &HView) {
        assert!(self.state.is_some(), "not mounted");
        view.pend_update();
    }

    /// Get the backing layer if mounted.
    pub fn layer(&self) -> Option<&pal::HLayer> {
        self.state.as_ref().map(|s| &s.layer)
    }

    /// Update the backing layer. The caller-supplied paint function is used
    /// to provide new layer contents if necessary.
    ///
    /// `visual_bounds` is a rectangle specified in the view's coordinate space,
    /// which clips and encloses the drawn contents. In this coordinate space,
    /// the frame of the view is specified as
    /// `Box2::with_size(Point2::new(0.0, 0.0), frame().size())`.
    pub fn update_layer(
        &mut self,
        wm: pal::Wm,
        view: &HView,
        wnd: HWndRef,
        visual_bounds: Box2<f32>,
        paint: impl FnOnce(&mut PaintContext<'_>),
    ) {
        let state = self.state.as_mut().expect("not mounted");

        let layer = &state.layer;

        let view_frame = view.global_frame();
        let dpi_scale = wnd.dpi_scale();

        // Calculate the new bitmap size
        let phys_vis_bounds = [
            Point2::new(
                (visual_bounds.min.x * dpi_scale).floor() as i32,
                (visual_bounds.min.y * dpi_scale).floor() as i32,
            ),
            Point2::new(
                (visual_bounds.max.x * dpi_scale).ceil() as i32,
                (visual_bounds.max.y * dpi_scale).ceil() as i32,
            ),
        ];
        let phys_vis_bounds = [
            phys_vis_bounds[0],
            Point2::new(
                max(phys_vis_bounds[0].x + 1, phys_vis_bounds[1].x),
                max(phys_vis_bounds[0].y + 1, phys_vis_bounds[1].y),
            ),
        ];
        let bmp_size: Vector2<i32> = phys_vis_bounds[1] - phys_vis_bounds[0];
        let bmp_size: [u32; 2] = bmp_size.cast::<u32>().unwrap().into();
        let bmp_pt_size = Vector2::from(bmp_size).cast::<f32>().unwrap() / dpi_scale;

        // (Re-)create the bitmap if needed
        let bmp = if Some(phys_vis_bounds) != state.last_phys_vis_bounds {
            let mut builder = pal::BitmapBuilder::new(bmp_size);

            // Configure the canvas to use the view's coordinate space
            builder.mult_transform(Matrix3::from_translation(vec2(
                -(phys_vis_bounds[0].x as f32),
                -(phys_vis_bounds[0].y as f32),
            )));

            // Apply DPI scaling
            builder.mult_transform(Matrix3::from_scale_2d(dpi_scale));

            // Call the paint function
            paint(&mut PaintContext {
                canvas: &mut builder,
                size: bmp_pt_size,
                dpi_scale,
            });

            state.last_phys_vis_bounds = Some(phys_vis_bounds);

            Some(builder.into_bitmap())
        } else {
            None
        };

        // Calculate the new layer bounds
        let bounds = Box2::new(
            phys_vis_bounds[0].cast::<f32>().unwrap() / dpi_scale,
            phys_vis_bounds[1].cast::<f32>().unwrap() / dpi_scale,
        )
        .translate(vec2(view_frame.min.x, view_frame.min.y));

        wm.set_layer_attr(
            layer,
            pal::LayerAttrs {
                contents: bmp.map(Some),
                bounds: Some(bounds),
                ..Default::default()
            },
        );
    }

    /// Update the backing layer. The layer will use 9-grid scaling to flexibly
    /// resize without re-painting.
    /// The caller-supplied paint function is used to provide new layer contents
    /// if necessary.
    ///
    /// The client must choose between `update_layer` and `update_layer_border`
    /// depending on whether the contents is eligible for 9-grid scaling or not.
    /// It's not allowed switch to the other one after calling one.
    ///
    /// `paint` draws the border image within the region
    /// `(-radius, -radius)-(radius, radius)`.
    pub fn update_layer_border(
        &mut self,
        wm: pal::Wm,
        view: &HView,
        wnd: HWndRef,
        radius: f32,
        paint: impl FnOnce(&mut PaintContext<'_>),
    ) {
        // TODO: Review this API. Perhaps this had better be merged into
        // `update_layer`. The usefulness of this API is questionable because it
        // gives up the opportunity of sharing a single pre-rendered image in
        // UI elements of the same kind, and such elements are likely to be the
        // main use cases of this API.
        let state = self.state.as_mut().expect("not mounted");

        let layer = &state.layer;

        let view_frame = view.global_frame();
        let dpi_scale = wnd.dpi_scale();

        let bmp_size = max((radius * dpi_scale) as u32, 1);

        // This value is used just for detecting size changes, not really
        // makes sense
        let phys_vis_bounds = [
            Point2::new(0, 0),
            Point2::new(bmp_size as i32, bmp_size as i32),
        ];

        let bmp_pt_size = bmp_size as f32 / dpi_scale;

        // (Re-)create the bitmap if needed
        let bmp = if Some(phys_vis_bounds) != state.last_phys_vis_bounds {
            let mut builder = pal::BitmapBuilder::new([bmp_size * 2, bmp_size * 2]);

            // Move the origin to the center of the bitmap
            builder.mult_transform(Matrix3::from_translation(vec2(
                bmp_size as f32,
                bmp_size as f32,
            )));

            // Apply DPI scaling
            builder.mult_transform(Matrix3::from_scale_2d(dpi_scale));

            // Call the paint function
            paint(&mut PaintContext {
                canvas: &mut builder,
                size: vec2(bmp_pt_size, bmp_pt_size) * 2.0,
                dpi_scale,
            });

            state.last_phys_vis_bounds = Some(phys_vis_bounds);

            Some(builder.into_bitmap())
        } else {
            None
        };

        wm.set_layer_attr(
            layer,
            pal::LayerAttrs {
                contents: bmp.map(Some),
                bounds: Some(view_frame),
                contents_scale: Some(dpi_scale),
                contents_center: Some(box2! { point: [0.5, 0.5] }),
                ..Default::default()
            },
        );
    }

    /// Implements [`ViewListener::update`] using a caller-supplied paint
    /// function.
    ///
    /// [`ViewListener::update`]: crate::uicore::ViewListener::update
    ///
    /// This method internally calls `UpdateCtx::set_layers`. If you need
    /// more control over a view's backing layers, you should use
    /// [`update_layer`] and [`layer`] instead.
    ///
    /// [`update_layer`]: CanvasMixin::update_layer
    /// [`layer`]: CanvasMixin::layer
    pub fn update(
        &mut self,
        wm: pal::Wm,
        view: &HView,
        ctx: &mut UpdateCtx<'_>,
        paint: impl FnOnce(&mut PaintContext<'_>),
    ) {
        let visual_bounds = Box2::with_size(Point2::new(0.0, 0.0), view.frame().size());

        self.update_layer(wm, view, ctx.hwnd(), visual_bounds, paint);

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![self.layer().unwrap().clone()]);
        }
    }

    /// Pend a redraw.
    ///
    /// This method updates an internal flag and calls [`HView::pend_update`].
    /// As a result, a caller-supplied paint function will be used to update
    /// the layer contents when `update` is called for the next time.
    ///
    /// [`HView::pend_update`]: crate::uicore::HView::pend_update
    pub fn pend_draw(&mut self, view: &HView) {
        if let Some(state) = &mut self.state {
            state.last_phys_vis_bounds = None;
            view.pend_update();
        }
    }
}
