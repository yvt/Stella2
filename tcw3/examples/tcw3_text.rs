use cggeom::{prelude::*, Box2};
use cgmath::{vec2, Point2};
use std::cell::RefCell;
use structopt::StructOpt;

use tcw3::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::{EmptyLayout, FillLayout},
        mixins::CanvasMixin,
    },
    uicore::{HView, HWnd, SizeTraits, UpdateCtx, ViewFlags, ViewListener, WndListener},
};

#[derive(StructOpt, Debug)]
#[structopt(name = "tcw3_text")]
struct Opt {
    /// Specify the displayed text
    #[structopt(short = "t", long = "text")]
    text: Option<String>,
}

struct MyViewListener {
    opt: Opt,
    canvas: RefCell<CanvasMixin>,
}

impl MyViewListener {
    fn new(opt: Opt) -> Self {
        Self {
            opt,
            canvas: RefCell::new(CanvasMixin::new()),
        }
    }
}

impl ViewListener for MyViewListener {
    fn mount(&self, wm: pal::WM, view: &HView, wnd: &HWnd) {
        self.canvas.borrow_mut().mount(wm, view, wnd);
        wm.set_layer_attr(
            self.canvas.borrow().layer().unwrap(),
            &pal::LayerAttrs {
                bg_color: Some(pal::RGBAF32::new(0.8, 0.8, 0.8, 1.0)),
                ..Default::default()
            },
        );
    }

    fn unmount(&self, wm: pal::WM, view: &HView) {
        self.canvas.borrow_mut().unmount(wm, view);
    }

    fn position(&self, wm: pal::WM, view: &HView) {
        self.canvas.borrow_mut().position(wm, view);
    }

    fn update(&self, wm: pal::WM, view: &HView, ctx: &mut UpdateCtx<'_>) {
        self.canvas.borrow_mut().update(wm, view, ctx, |draw_ctx| {
            let size = draw_ctx.size;
            let c = &mut draw_ctx.canvas;

            let char_style = pal::CharStyle::new(pal::CharStyleAttrs {
                ..Default::default()
            });
            let wrap_width = size.x - 20.0;
            let text = if let Some(custom_text) = &self.opt.text {
                &custom_text[..]
            } else {
                lipsum::LOREM_IPSUM
            };
            let text_layout = pal::TextLayout::from_text(text, &char_style, Some(wrap_width));
            c.draw_text(
                &text_layout,
                Point2::new(10.0, 10.0),
                pal::RGBAF32::new(0.0, 0.0, 0.0, 1.0),
            );

            // Draw text layout outline
            c.set_stroke_rgb(pal::RGBAF32::new(0.5, 0.5, 0.5, 1.0));
            c.stroke_rect(Box2::with_size(
                Point2::new(10.0, 10.0),
                vec2(wrap_width, size.y),
            ));
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
    // Parse command-line arguments
    let opt = Opt::from_args();

    let wm = pal::WM::global();

    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);
    wnd.set_listener(Box::new(MyWndListener));

    let v = HView::new(ViewFlags::empty());
    v.set_listener(Box::new(MyViewListener::new(opt)));
    v.set_layout(Box::new(EmptyLayout::new(SizeTraits {
        preferred: vec2(320.0, 180.0),
        ..Default::default()
    })));

    wnd.content_view().set_layout(Box::new(FillLayout::new(v)));

    wm.enter_main_loop();
}
