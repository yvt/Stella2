use tcw3::pal::traits::*;

fn main() {
    let wm = tcw3::pal::wm();
    let wnd = wm.new_wnd(&tcw3::pal::types::WndAttrs {
        size: None,
        caption: Some("Hello world"),
        visible: Some(true),
    });
    wm.enter_main_loop();
}
