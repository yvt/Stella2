use tcw3::pal::{self, prelude::*};

mod crashhandler;
mod model;
mod view;

fn main() {
    crashhandler::init();

    let wm = pal::WM::global();

    let _view = self::view::AppView::new(wm);

    wm.enter_main_loop();
}
