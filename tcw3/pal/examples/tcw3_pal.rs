use cggeom::{box2, prelude::*};
use cgmath::Point2;
use tcw3_pal::{self as pal, prelude::*};

struct Listener {
    flex_layer: pal::HLayer,
}

impl WndListener<pal::Wm> for Listener {
    fn dpi_scale_changed(&self, wm: pal::Wm, wnd: &pal::HWnd) {
        dbg!(wm.get_wnd_dpi_scale(wnd));
    }

    fn close(&self, wm: pal::Wm, _: &pal::HWnd) {
        wm.terminate();
    }

    fn resize(&self, wm: pal::Wm, hwnd: &pal::HWnd) {
        let [w, h] = wm.get_wnd_size(hwnd);
        wm.set_layer_attr(
            &self.flex_layer,
            pal::LayerAttrs {
                bounds: Some(box2! {
                    min: [20.0, 120.0],
                    max: [w as f32 - 20.0, h as f32 - 20.0],
                }),
                ..Default::default()
            },
        );
        wm.update_wnd(hwnd);
    }

    fn mouse_motion(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>) {
        println!("mouse_motion {:?}", loc);
    }

    fn mouse_leave(&self, _: pal::Wm, _: &pal::HWnd) {
        println!("mouse_leave");
    }

    fn mouse_drag(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn MouseDragListener<pal::Wm>> {
        println!("mouse_drag {:?}", (loc, button));
        Box::new(DragListener)
    }
}

struct DragListener;

impl MouseDragListener<pal::Wm> for DragListener {
    fn mouse_motion(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>) {
        println!("drag: mouse_motion {:?}", loc);
    }
    fn mouse_down(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        println!("drag: mouse_down {:?}", (loc, button));
    }
    fn mouse_up(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        println!("drag: mouse_up {:?}", (loc, button));
    }
    fn cancel(&self, _: pal::Wm, _: &pal::HWnd) {
        println!("drag: cancel");
    }
}

fn main() {
    let wm = pal::Wm::global();

    let mut bmp_builder = pal::BitmapBuilder::new([100, 100]);
    bmp_builder.move_to(Point2::new(20.0, 20.0));
    bmp_builder.line_to(Point2::new(80.0, 20.0));
    bmp_builder.line_to(Point2::new(20.0, 80.0));
    bmp_builder.line_to(Point2::new(80.0, 80.0));
    bmp_builder.quad_bezier_to(Point2::new(80.0, 20.0), Point2::new(20.0, 20.0));
    bmp_builder.stroke();

    let bmp = bmp_builder.into_bitmap();

    let layer2 = wm.new_layer(pal::LayerAttrs {
        bounds: Some(box2! { min: [20.0, 120.0], max: [150.0, 250.0] }),
        bg_color: Some(pal::RGBAF32::new(0.5, 0.8, 0.5, 1.0)),
        contents: Some(Some(bmp.clone())),
        ..Default::default()
    });

    let layer = wm.new_layer(pal::LayerAttrs {
        bounds: Some(box2! { min: [20.0, 20.0], max: [200.0, 100.0] }),
        bg_color: Some(pal::RGBAF32::new(0.8, 0.5, 0.5, 1.0)),
        contents: Some(Some(bmp)),
        sublayers: Some(vec![layer2.clone()]),
        transform: Some(cgmath::Matrix3::from_angle(cgmath::Deg(3.0))),
        ..Default::default()
    });

    let wnd = wm.new_wnd(pal::WndAttrs {
        caption: Some("Hello world".into()),
        visible: Some(true),
        layer: Some(Some(layer)),
        size: Some([220, 270]),
        min_size: Some([220, 270]),
        listener: Some(Box::new(Listener { flex_layer: layer2 })),
        flags: Some(pal::WndFlags::default()),
        ..Default::default()
    });

    wm.update_wnd(&wnd);
    wm.enter_main_loop();
}
