use tcw3::{
    pal,
    pal::prelude::*,
    ui::{
        layouts::TableLayout,
        theming,
        views::{Label, Scrollbar},
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

    let mut scrollbar = Scrollbar::new(style_manager, false);

    let cells = vec![
        (label.view().clone(), [0, 0], AlignFlags::JUSTIFY),
        (scrollbar.view().clone(), [0, 1], AlignFlags::JUSTIFY),
    ];

    wnd.content_view()
        .set_layout(TableLayout::new(cells).with_uniform_margin(20.0));

    wm.enter_main_loop();
}
