use tcw3::pal::{self, prelude::*};

mod crashhandler;

fn main() {
    crashhandler::init();

    let wm = pal::WM::global();
    wm.enter_main_loop();
}
