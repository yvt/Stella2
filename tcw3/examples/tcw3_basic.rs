use cgmath::{vec2, Point2};
use std::cell::RefCell;

use tcw3::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::{EmptyLayout, FillLayout},
        mixins::CanvasMixin,
    },
    uicore::{HView, HWnd, SizeTraits, UpdateCtx, ViewFlags, ViewListener, WndListener},
};

struct MyViewListener {
    canvas: RefCell<CanvasMixin>,
}

impl MyViewListener {
    fn new() -> Self {
        Self {
            canvas: RefCell::new(CanvasMixin::new()),
        }
    }
}

impl ViewListener for MyViewListener {
    fn mount(&self, wm: pal::WM, view: &HView, wnd: &HWnd) {
        dbg!();
        self.canvas.borrow_mut().mount(wm, view, wnd);
        wm.set_layer_attr(
            self.canvas.borrow().layer().unwrap(),
            &pal::LayerAttrs {
                bg_color: Some(pal::RGBAF32::new(0.5, 0.8, 0.5, 1.0)),
                ..Default::default()
            },
        );
    }

    fn unmount(&self, wm: pal::WM, view: &HView) {
        dbg!();
        self.canvas.borrow_mut().unmount(wm, view);
    }

    fn position(&self, wm: pal::WM, view: &HView) {
        self.canvas.borrow_mut().position(wm, view);
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        self.canvas.borrow_mut().update(wm, view, ctx, |draw_ctx| {
            let c = &mut draw_ctx.canvas;
            let size = draw_ctx.size;

            c.move_to(Point2::new(size.x * 0.2, size.y * 0.2));
            c.line_to(Point2::new(size.x * 0.8, size.y * 0.2));
            c.line_to(Point2::new(size.x * 0.2, size.y * 0.8));
            c.line_to(Point2::new(size.x * 0.8, size.y * 0.8));
            c.quad_bezier_to(
                Point2::new(size.x * 0.8, size.y * 0.2),
                Point2::new(size.x * 0.2, size.y * 0.2),
            );
            c.stroke();

            let char_style = pal::CharStyle::new(pal::CharStyleAttrs {
                ..Default::default()
            });
            let text_layout =
                pal::TextLayout::from_text(lipsum::LOREM_IPSUM, &char_style, Some(size.x - 20.0));
            c.draw_text(
                &text_layout,
                Point2::new(10.0, 10.0),
                pal::RGBAF32::new(0.0, 0.0, 0.4, 1.0),
            );
        });
    }
}

struct MyWndListener;

impl WndListener for MyWndListener {
    fn close(&self, wm: pal::WM, _: &HWnd) {
        wm.terminate();
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
    wnd.set_listener(Box::new(MyWndListener));

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
