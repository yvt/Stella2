use cgmath::{vec2, Point2};
use std::cell::RefCell;

use tcw3::{
    pal,
    pal::prelude::*,
    ui::layouts::{EmptyLayout, FillLayout},
    uicore::{HView, HWnd, SizeTraits, UpdateCtx, ViewFlags, ViewListener},
};

struct MyViewListener {
    layer: RefCell<Option<pal::HLayer>>,
}

impl MyViewListener {
    fn new() -> Self {
        Self {
            layer: RefCell::new(None),
        }
    }
}

impl ViewListener for MyViewListener {
    fn mount(&self, wm: &pal::WM, view: &HView) {
        let mut bmp_builder = pal::BitmapBuilder::new([100, 100]);
        bmp_builder.move_to(Point2::new(20.0, 20.0));
        bmp_builder.line_to(Point2::new(80.0, 20.0));
        bmp_builder.line_to(Point2::new(20.0, 80.0));
        bmp_builder.line_to(Point2::new(80.0, 80.0));
        bmp_builder.quad_bezier_to(Point2::new(80.0, 20.0), Point2::new(20.0, 20.0));
        bmp_builder.stroke();

        let bmp = bmp_builder.into_bitmap();

        *self.layer.borrow_mut() = Some(wm.new_layer(&pal::LayerAttrs {
            bg_color: Some(pal::RGBAF32::new(0.5, 0.8, 0.5, 1.0)),
            contents: Some(Some(bmp.clone())),
            ..Default::default()
        }));

        view.pend_update();
    }

    fn unmount(&self, _: &pal::WM, _: &HView) {
        if let Some(hlayer) = self.layer.borrow_mut().take() {
            wm.remove_layer(&hlayer);
        }
    }

    fn position(&self, _: &pal::WM, view: &HView) {
        view.pend_update();
    }

    fn update(&self, wm: &pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.borrow();
        let layer = layer.as_ref().unwrap();

        wm.set_layer_attr(
            &layer,
            &pal::LayerAttrs {
                bounds: Some(view.global_frame()),
                ..Default::default()
            },
        );

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![(*layer).clone()]);
        }
    }
}

fn main() {
    let wm = pal::WM::global();

    pal::WM::invoke_on_main_thread(|_| {
        // The following statement panics if we are not on the main thread
        pal::WM::global();
    });

    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);

    let subview = HView::new(ViewFlags::empty());
    subview.set_listener(Box::new(MyViewListener::new()));
    subview.set_layout(Box::new(EmptyLayout::new(SizeTraits {
        min: vec2(100.0, 100.0),
        preferred: vec2(320.0, 180.0),
        max: vec2(1280.0, 720.0),
    })));

    wnd.content_view()
        .set_layout(Box::new(FillLayout::with_uniform_margin(subview, 10.0)));

    wm.enter_main_loop();
}
