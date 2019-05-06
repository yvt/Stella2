use cggeom::{prelude::*, Box2};
use cgmath::Point2;
use tcw3::pal::{self, prelude::*};

struct Listener {
    flex_layer: pal::HLayer,
}

impl WndListener<pal::WM> for Listener {
    fn dpi_scale_changed(&self, wm: pal::WM, wnd: &pal::HWnd) {
        dbg!(wm.get_wnd_dpi_scale(wnd));
    }

    fn close(&self, wm: pal::WM, _: &pal::HWnd) {
        wm.terminate();
    }

    fn resize(&self, wm: pal::WM, hwnd: &pal::HWnd) {
        let [w, h] = wm.get_wnd_size(hwnd);
        wm.set_layer_attr(
            &self.flex_layer,
            &pal::LayerAttrs {
                bounds: Some(Box2::new(
                    Point2::new(20.0, 120.0),
                    Point2::new(w as f32 - 20.0, h as f32 - 20.0),
                )),
                ..Default::default()
            },
        );
        wm.update_wnd(hwnd);
    }
}

fn main() {
    let wm = pal::WM::global();

    let mut bmp_builder = pal::BitmapBuilder::new([100, 100]);
    bmp_builder.move_to(Point2::new(20.0, 20.0));
    bmp_builder.line_to(Point2::new(80.0, 20.0));
    bmp_builder.line_to(Point2::new(20.0, 80.0));
    bmp_builder.line_to(Point2::new(80.0, 80.0));
    bmp_builder.quad_bezier_to(Point2::new(80.0, 20.0), Point2::new(20.0, 20.0));
    bmp_builder.stroke();

    let bmp = bmp_builder.into_bitmap();

    let layer2 = wm.new_layer(&pal::LayerAttrs {
        bounds: Some(Box2::new(
            Point2::new(20.0, 120.0),
            Point2::new(150.0, 250.0),
        )),
        bg_color: Some(pal::RGBAF32::new(0.5, 0.8, 0.5, 1.0)),
        contents: Some(Some(bmp.clone())),
        ..Default::default()
    });

    let layer = wm.new_layer(&pal::LayerAttrs {
        bounds: Some(Box2::new(
            Point2::new(20.0, 20.0),
            Point2::new(200.0, 100.0),
        )),
        bg_color: Some(pal::RGBAF32::new(0.8, 0.5, 0.5, 1.0)),
        contents: Some(Some(bmp)),
        sublayers: Some(vec![layer2.clone()]),
        transform: Some(cgmath::Matrix3::from_angle(cgmath::Deg(3.0))),
        ..Default::default()
    });

    let wnd = wm.new_wnd(&pal::WndAttrs {
        caption: Some("Hello world"),
        visible: Some(true),
        layer: Some(Some(layer)),
        size: Some([220, 270]),
        min_size: Some([220, 270]),
        listener: Some(Some(std::rc::Rc::new(Listener { flex_layer: layer2 }))),
        flags: Some(pal::WndFlags::default()),
        ..Default::default()
    });

    wm.update_wnd(&wnd);
    wm.enter_main_loop();
}
