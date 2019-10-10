use std::{cell::RefCell, rc::Rc};
use tcw3::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::TableLayout,
        theming,
        views::{scrollbar::ScrollbarDragListener, Label, Scrollbar},
        AlignFlags,
    },
    uicore::{HWnd, WndListener},
};

struct MyWndListener;

impl WndListener for MyWndListener {
    fn close(&self, wm: pal::Wm, _: &HWnd) {
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

    let mut label = Label::new(style_manager);
    label.set_text("Hello, world! «coi ro do .ui» Saluton! nuqneH");

    let scrollbar = Scrollbar::new(style_manager, false);
    let scrollbar = Rc::new(RefCell::new(scrollbar));
    {
        let scrollbar_weak = Rc::downgrade(&scrollbar);
        scrollbar.borrow_mut().set_on_drag(move |_| {
            let scrollbar = scrollbar_weak.upgrade().unwrap();
            let value = scrollbar.borrow().value();
            Box::new(MyScrollbarDragListener { value, scrollbar })
        });
    }

    let cells = vec![
        (label.view().clone(), [0, 0], AlignFlags::JUSTIFY),
        (
            scrollbar.borrow().view().clone(),
            [0, 1],
            AlignFlags::JUSTIFY,
        ),
    ];

    wnd.content_view()
        .set_layout(TableLayout::new(cells).with_uniform_margin(20.0));

    wm.enter_main_loop();
}

struct MyScrollbarDragListener {
    scrollbar: Rc<RefCell<Scrollbar>>,
    value: f64,
}

impl ScrollbarDragListener for MyScrollbarDragListener {
    fn motion(&self, _: pal::Wm, new_value: f64) {
        self.scrollbar.borrow_mut().set_value(new_value);
    }

    fn cancel(&self, _: pal::Wm) {
        self.scrollbar.borrow_mut().set_value(self.value);
    }
}
