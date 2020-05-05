use cggeom::box2;
use cgmath::{vec2, Point2};
use flags_macro::flags;
use std::cell::RefCell;
use tcw3::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::{AbsLayout, TableLayout},
        mixins::CanvasMixin,
        theming,
        views::Label,
        AlignFlags,
    },
    uicore::{
        HView, HViewRef, HWnd, HWndRef, SizeTraits, UpdateCtx, ViewFlags, ViewListener, WndListener,
    },
};

struct MyViewListener {
    size_traits: SizeTraits,
    canvas: RefCell<CanvasMixin>,
}

impl MyViewListener {
    fn new(size_traits: SizeTraits) -> Self {
        Self {
            size_traits,
            canvas: RefCell::new(CanvasMixin::new()),
        }
    }
}

impl ViewListener for MyViewListener {
    fn mount(&self, wm: pal::Wm, view: HViewRef<'_>, wnd: HWndRef<'_>) {
        self.canvas.borrow_mut().mount(wm, view, wnd);
        wm.set_layer_attr(
            self.canvas.borrow().layer().unwrap(),
            pal::LayerAttrs::default(),
        );
    }

    fn unmount(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.canvas.borrow_mut().unmount(wm, view);
    }

    fn position(&self, wm: pal::Wm, view: HViewRef<'_>) {
        self.canvas.borrow_mut().position(wm, view);
    }

    fn update(&self, wm: pal::Wm, view: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        self.canvas.borrow_mut().update(wm, view, ctx, |draw_ctx| {
            let c = &mut draw_ctx.canvas;

            let size = draw_ctx.size;

            c.set_fill_rgb(pal::RGBAF32::new(0.3, 0.9, 0.3, 0.3));
            c.fill_rect(box2! {
                top_left: [0.0, 0.0].into(),
                size: self.size_traits.preferred.into(),
            });

            c.set_fill_rgb(pal::RGBAF32::new(0.9, 0.3, 0.3, 0.8));
            c.fill_rect(box2! {
                top_left: [0.0, 0.0].into(),
                size: self.size_traits.min.into(),
            });

            c.stroke_rect(box2! { min: [0.5, 0.5], max: [size.x - 0.5, size.y - 0.5] });
        });
    }
}

struct MyWndListener;

impl WndListener for MyWndListener {
    fn close(&self, wm: pal::Wm, _: HWndRef<'_>) {
        wm.terminate();
    }
}

fn main() {
    env_logger::init();

    let wm = pal::Wm::global();
    let style_manager = theming::Manager::global(wm);

    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);
    wnd.set_listener(MyWndListener);

    let cells = (0..16).map(|i| {
        let col = i % 4;
        let row = i / 4;

        let view = HView::new(ViewFlags::default());
        let size_traits = SizeTraits {
            min: vec2(20.0, 20.0),
            max: vec2(100.0, 100.0),
            preferred: vec2((col + 1) as f32 * 20.0, (row + 1) as f32 * 20.0),
        };

        view.set_listener(MyViewListener::new(size_traits));

        let label = Label::new(style_manager);
        label.set_text(format!(
            "[{}, {}]",
            size_traits.preferred.x, size_traits.preferred.y
        ));

        view.set_layout(AbsLayout::new(
            size_traits,
            Some((
                label.view(),
                box2! { point: Point2::new(5.0, 5.0) },
                flags![AlignFlags::{LEFT | TOP}],
            )),
        ));

        (
            view,
            [col, row],
            flags![AlignFlags::{VERT_CENTER | HORZ_JUSTIFY}],
        )
    });

    wnd.content_view().set_layout(
        TableLayout::new(cells)
            .with_uniform_margin(20.0)
            .with_uniform_spacing(10.0),
    );

    wm.enter_main_loop();
}
