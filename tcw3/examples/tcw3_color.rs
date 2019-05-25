use cggeom::{prelude::*, Box2};
use cgmath::{vec2, Point2};
use std::cell::RefCell;
use tcw3::{
    pal,
    pal::prelude::*,
    ui::{layouts::AbsLayout, mixins::CanvasMixin},
    uicore::{
        HView, HWnd, SizeTraits, UpdateCtx, ViewFlags, ViewListener, WndListener, WndStyleFlags,
    },
};

struct MyViewListener {
    color: pal::RGBAF32,
    canvas: RefCell<CanvasMixin>,
}

impl MyViewListener {
    fn new(color: pal::RGBAF32) -> Self {
        Self {
            color,
            canvas: RefCell::new(CanvasMixin::new()),
        }
    }
}

impl ViewListener for MyViewListener {
    fn mount(&self, wm: pal::WM, view: &HView, wnd: &HWnd) {
        self.canvas.borrow_mut().mount(wm, view, wnd);
        wm.set_layer_attr(
            self.canvas.borrow().layer().unwrap(),
            pal::LayerAttrs {
                bg_color: Some(self.color),
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
            let c = &mut draw_ctx.canvas;

            let size = draw_ctx.size;

            // The left half is filled using `fill_rect`. The right half is not
            // painted, revealing the background color of the layer.
            // The both halves should display the same color.
            c.set_fill_rgb(self.color);
            c.fill_rect(Box2::new(
                Point2::new(0.0, 0.0),
                Point2::new(size.x * 0.5, size.y),
            ));

            c.set_stroke_rgb(pal::RGBAF32::new(0.0, 0.0, 0.0, 1.0));
            c.stroke_rect(Box2::new(
                Point2::new(0.5, 0.5),
                Point2::new(size.x - 0.5, size.y - 0.5),
            ));
            c.stroke_rect(Box2::new(
                Point2::new(0.5, 0.5),
                Point2::new(size.x * 0.5 - 0.5, size.y - 0.5),
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
    let wm = pal::WM::global();

    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);
    wnd.set_style_flags(WndStyleFlags::default() - WndStyleFlags::RESIZABLE);
    wnd.set_listener(MyWndListener);

    const CELL_W: f32 = 50.0;
    const CELL_H: f32 = 20.0;
    const MARGIN: f32 = 10.0;

    let views: Vec<_> = (0..30)
        .map(|i| {
            let row = i / 3;
            let col = i % 3;

            let luminance = row as f32 / 9.0;
            let color = pal::RGBAF32::new(
                luminance * (col == 0) as u32 as f32,
                luminance * (col == 1) as u32 as f32,
                luminance * (col == 2) as u32 as f32,
                1.0,
            );

            let subview = HView::new(ViewFlags::default());
            subview.set_listener(MyViewListener::new(color));

            let frame = Box2::with_size(
                Point2::new(CELL_W * col as f32 + MARGIN, CELL_H * row as f32 + MARGIN),
                vec2(CELL_W, CELL_H),
            );

            (subview, frame)
        })
        .collect();

    let size = vec2(CELL_W * 3.0 + MARGIN * 2.0, CELL_H * 10.0 + MARGIN * 2.0);

    wnd.content_view().set_layout(AbsLayout::new(
        SizeTraits {
            min: size,
            max: size,
            preferred: size,
        },
        views,
    ));

    wm.enter_main_loop();
}
