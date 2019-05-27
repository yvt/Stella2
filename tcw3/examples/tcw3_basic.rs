use tcw3::{
    pal,
    pal::prelude::*,
    ui::{layouts::FillLayout, views::Label},
    uicore::{HWnd, WndListener},
};

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
    wnd.set_listener(MyWndListener);

    let mut label = Label::new();
    label.set_text("Hello, world! «coi ro do .ui» Saluton! nuqneH");

    wnd.content_view()
        .set_layout(FillLayout::new(label.view().clone()).with_uniform_margin(20.0));

    wm.enter_main_loop();
}
