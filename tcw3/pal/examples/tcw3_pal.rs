use cggeom::{box2, prelude::*};
use cgmath::{Point2, Vector2};
use log::info;
use tcw3_pal::{self as pal, prelude::*};

struct Listener {
    flex_layer: pal::HLayer,
}

impl WndListener<pal::Wm> for Listener {
    fn dpi_scale_changed(&self, wm: pal::Wm, wnd: &pal::HWnd) {
        info!("dpi_scale_changed {:?}", wm.get_wnd_dpi_scale(wnd));
    }

    fn close_requested(&self, wm: pal::Wm, _: &pal::HWnd) {
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
        info!("mouse_motion {:?}", loc);
    }

    fn mouse_leave(&self, _: pal::Wm, _: &pal::HWnd) {
        info!("mouse_leave");
    }

    fn mouse_drag(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        loc: Point2<f32>,
        button: u8,
    ) -> Box<dyn MouseDragListener<pal::Wm>> {
        info!("mouse_drag {:?}", (loc, button));
        Box::new(DragListener)
    }

    fn scroll_motion(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>, delta: &pal::ScrollDelta) {
        info!("scroll_motion {:?} {:?}", loc, delta);
    }

    fn scroll_gesture(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        loc: Point2<f32>,
    ) -> Box<dyn ScrollListener<pal::Wm>> {
        info!("scroll_gesture {:?}", loc);
        Box::new(MyScrollListener)
    }
}

struct DragListener;

impl MouseDragListener<pal::Wm> for DragListener {
    fn mouse_motion(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>) {
        info!("drag: mouse_motion {:?}", loc);
    }
    fn mouse_down(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        info!("drag: mouse_down {:?}", (loc, button));
    }
    fn mouse_up(&self, _: pal::Wm, _: &pal::HWnd, loc: Point2<f32>, button: u8) {
        info!("drag: mouse_up {:?}", (loc, button));
    }
    fn cancel(&self, _: pal::Wm, _: &pal::HWnd) {
        info!("drag: cancel");
    }
}

struct MyScrollListener;

impl ScrollListener<pal::Wm> for MyScrollListener {
    fn motion(&self, _: pal::Wm, _: &pal::HWnd, delta: &pal::ScrollDelta, velocity: Vector2<f32>) {
        info!("scroll: motion {:?} {:?}", delta, velocity);
    }

    fn start_momentum_phase(&self, _: pal::Wm, _: &pal::HWnd) {
        info!("scroll: start_momentum_phase");
    }

    fn end(&self, _: pal::Wm, _: &pal::HWnd) {
        info!("scroll: end");
    }

    fn cancel(&self, _: pal::Wm, _: &pal::HWnd) {
        info!("scroll: cancel");
    }
}

fn main() {
    env_logger::init();

    let wm = pal::Wm::global();

    let mut bmp_builder = pal::BitmapBuilder::new([100, 100]);
    bmp_builder.set_stroke_rgb([0.0, 0.0, 0.0, 1.0].into());
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
