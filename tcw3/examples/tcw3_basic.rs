use tcw3::{
    pal,
    pal::prelude::*,
    ui::{layouts::FillLayout, theming, views::Label},
    uicore::{HWnd, WndListener},
};

struct MyWndListener;

impl WndListener for MyWndListener {
    fn close(&self, wm: pal::Wm, _: &HWnd) {
        wm.terminate();
    }
}

fn main() {
    pretty_env_logger::init();

    let wm = pal::Wm::global();
    let style_manager = theming::Manager::global(wm);

    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);
    wnd.set_listener(MyWndListener);

    let mut label = Label::new(style_manager);
    label.set_text("Hello, world! «coi ro do .ui» Saluton! nuqneH");

    wnd.content_view()
        .set_layout(FillLayout::new(label.view().clone()).with_uniform_margin(20.0));

    wm.enter_main_loop();
}
