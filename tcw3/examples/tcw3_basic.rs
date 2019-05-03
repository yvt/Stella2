use tcw3::{pal, pal::prelude::*, uicore};

fn main() {
    let wm = pal::wm();

    let wnd = uicore::HWnd::new(wm);
    wnd.set_visibility(true);

    wm.enter_main_loop();
}
