use cggeom::{prelude::*, Box2};
use cgmath::Point2;
use tcw3::pal::traits::*;

fn main() {
    let wm = tcw3::pal::wm();

    let layer = wm.new_layer(&tcw3::pal::types::LayerAttrs {
        bounds: Some(Box2::new(
            Point2::new(20.0, 20.0),
            Point2::new(200.0, 100.0),
        )),
        bg_color: Some(tcw3::pal::RGBAF32::new(0.8, 0.5, 0.5, 1.0)),
        ..Default::default()
    });

    let wnd = wm.new_wnd(&tcw3::pal::types::WndAttrs {
        caption: Some("Hello world"),
        visible: Some(true),
        layer: Some(Some(layer)),
        ..Default::default()
    });

    wm.update_wnd(&wnd);
    wm.enter_main_loop();
}
