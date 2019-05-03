use tcw3::pal::{self, prelude::*};

fn main() {
    let wm = pal::WM::global();
    wm.enter_main_loop();
}
