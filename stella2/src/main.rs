use cggeom::{prelude::*, Box2};
use cgmath::Point2;
use tcw3::pal::traits::*;

fn main() {
    let wm = tcw3::pal::wm();

    let bmp_builder = tcw3::pal::BitmapBuilder::new([256, 256]);
    let bmp = bmp_builder.into_bitmap();

    let layer2 = wm.new_layer(&tcw3::pal::types::LayerAttrs {
        bounds: Some(Box2::new(
            Point2::new(20.0, 120.0),
            Point2::new(150.0, 180.0),
        )),
        bg_color: Some(tcw3::pal::RGBAF32::new(0.5, 0.8, 0.5, 1.0)),
        contents: Some(Some(bmp.clone())),
        ..Default::default()
    });

    let layer = wm.new_layer(&tcw3::pal::types::LayerAttrs {
        bounds: Some(Box2::new(
            Point2::new(20.0, 20.0),
            Point2::new(200.0, 100.0),
        )),
        bg_color: Some(tcw3::pal::RGBAF32::new(0.8, 0.5, 0.5, 1.0)),
        contents: Some(Some(bmp)),
        sublayers: Some(vec![layer2]),
        transform: Some(cgmath::Matrix4::from_angle_z(cgmath::Deg(3.0))),
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
