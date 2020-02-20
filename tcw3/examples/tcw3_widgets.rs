use std::rc::Rc;
use tcw3::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::TableLayout,
        theming,
        views::{scrollbar::ScrollbarDragListener, Label, Scrollbar},
        AlignFlags,
    },
    uicore::{HWnd, HWndRef, WndListener},
};

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

    let label = Label::new(style_manager);
    label.set_text("Hello, world! «coi ro do .ui» Saluton! nuqneH");

    let scrollbar = Scrollbar::new(style_manager, false);
    let scrollbar = Rc::new(scrollbar);
    {
        let scrollbar_weak = Rc::downgrade(&scrollbar);
        scrollbar.set_on_drag(move |_| {
            let scrollbar = scrollbar_weak.upgrade().unwrap();
            let value = scrollbar.value();
            Box::new(MyScrollbarDragListener { value, scrollbar })
        });
    }
    {
        let scrollbar_weak = Rc::downgrade(&scrollbar);
        scrollbar.set_on_page_step(move |_, dir| {
            let scrollbar = scrollbar_weak.upgrade().unwrap();
            let value = scrollbar.value() + dir as i8 as f64 * scrollbar.page_step();
            scrollbar.set_value(value.max(0.0).min(1.0));
        });
    }

    let cells = vec![
        (label.view(), [0, 0], AlignFlags::JUSTIFY),
        (scrollbar.view(), [0, 1], AlignFlags::JUSTIFY),
    ];

    wnd.content_view()
        .set_layout(TableLayout::new(cells).with_uniform_margin(20.0));

    wm.enter_main_loop();
}

struct MyScrollbarDragListener {
    scrollbar: Rc<Scrollbar>,
    value: f64,
}

impl ScrollbarDragListener for MyScrollbarDragListener {
    fn motion(&self, _: pal::Wm, new_value: f64) {
        self.scrollbar.set_value(new_value);
    }

    fn cancel(&self, _: pal::Wm) {
        self.scrollbar.set_value(self.value);
    }
}
