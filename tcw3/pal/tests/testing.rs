use cggeom::{box2, prelude::*};
use cgmath::{Deg, Matrix3, Point2};
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    thread::spawn,
    time::{Duration, Instant},
};
use tcw3_pal::{self as pal, iface::Wm as _, prelude::*, testing, MtLock, Wm};

#[test]
fn create_testing_wm() {
    testing::run_test(|twm| {
        // This block might or might not run depending on a feature flag
        twm.step_until(Instant::now() + Duration::from_millis(100));
    });
}

#[test]
fn invoke() {
    testing::run_test(|twm| {
        let flag = Rc::new(Cell::new(false));
        {
            let flag = Rc::clone(&flag);
            twm.wm().invoke(move |_| flag.set(true));
        }

        // Wait until the closure is called and the flag is set
        while !flag.get() {
            twm.step();
        }
    });
}

#[test]
fn step_unsend() {
    testing::run_test(|twm| {
        let flag = Rc::new(Cell::new(false));
        {
            let flag = Rc::clone(&flag);
            twm.wm().invoke(move |_| flag.set(true));
        }

        assert!(!flag.get());
        twm.step_unsend();
        assert!(flag.get());
    });
}

#[test]
fn invoke_on_main_thread() {
    testing::run_test(|twm| {
        let flag = Arc::new(MtLock::<_, Wm>::new(Cell::new(false)));

        {
            let flag = Arc::clone(&flag);
            spawn(move || {
                Wm::invoke_on_main_thread(move |wm| flag.get_with_wm(wm).set(true));
            });
        }

        // Wait until the closure is called and the flag is set
        while !flag.get_with_wm(twm.wm()).get() {
            twm.step();
        }
    });
}

#[test]
#[should_panic]
fn panicking() {
    testing::run_test(|twm| {
        panic!("this panic should be contained to this test case");
    });
}

#[test]
fn bitmap_size() {
    testing::run_test(|_| {
        const SIZE: [u32; 2] = [12, 34];
        let bitmap = pal::BitmapBuilder::new(SIZE).into_bitmap();
        assert_eq!(bitmap.size(), SIZE);
    });
}

/// Execute drawing commands on a `BitmapBuilder` and see if it doesn't panic.
/// The rendering result is not checked because there is currently no API to
/// get a bitmap content. (When we do, I'd like to see conformance tests for all
/// backends.)
#[test]
fn bitmap_canvas() {
    testing::run_test(|_| {
        let mut b = pal::BitmapBuilder::new([12, 34]);
        b.save();

        b.set_fill_rgb([0.2, 0.3, 0.4, 0.6].into());
        b.set_stroke_rgb([0.2, 0.3, 0.4, 0.6].into());

        b.set_line_cap(pal::LineCap::Butt);
        b.set_line_join(pal::LineJoin::Miter);
        b.set_line_dash(0.5, &[1.0, 2.0, 3.0, 4.0]);
        b.set_line_width(1.5);
        b.set_line_miter_limit(3.0);

        b.mult_transform(Matrix3::from_angle(Deg(30.0)));

        b.begin_path();
        b.move_to([5.0, 6.0].into());
        b.line_to([6.0, 6.0].into());
        b.cubic_bezier_to(
            [20.0, 21.0].into(),
            [10.0, 11.0].into(),
            [10.0, 16.0].into(),
        );
        b.quad_bezier_to([20.0, 21.0].into(), [10.0, 16.0].into());
        b.close_path();
        b.fill();
        b.stroke();
        b.clip();

        b.restore();
    });
}

#[test]
fn char_style() {
    testing::run_test(|_| {
        let attrs = pal::CharStyleAttrs {
            sys: Some(pal::SysFontType::SmallEmph),
            ..Default::default()
        };
        let _ = pal::CharStyle::new(attrs);
    });
}

#[test]
fn bitmap_text() {
    testing::run_test(|_| {
        let mut b = pal::BitmapBuilder::new([12, 34]);

        let style = pal::CharStyle::new(Default::default());

        let layout = pal::TextLayout::from_text("20% cooler", &style, None);

        dbg!(layout.layout_bounds());
        dbg!(layout.visual_bounds());

        b.draw_text(&layout, [10.0, 10.0].into(), [0.3, 0.5, 0.6, 1.0].into());
    });
}

#[test]
fn run_test_reset() {
    testing::run_test(|twm| {
        twm.wm().new_wnd(Default::default());
        assert_eq!(twm.hwnds().len(), 1);
    });
    // The backend is automatically reset between test runs
    testing::run_test(|twm| {
        twm.wm().new_wnd(Default::default());
        assert_eq!(twm.hwnds().len(), 1);
    });
}

#[test]
fn empty_wnd() {
    testing::run_test(|twm| {
        let wm = twm.wm();

        const CAPTION: &str = "Hello world";
        const VIS: bool = true;
        const SIZE: [u32; 2] = [100, 200];
        const MIN_SIZE: [u32; 2] = [50, 100];
        const MAX_SIZE: [u32; 2] = [300, 300];

        let hwnd = wm.new_wnd(pal::WndAttrs {
            caption: Some(CAPTION.into()),
            visible: Some(VIS),
            size: Some(SIZE),
            min_size: Some(MIN_SIZE),
            max_size: Some(MAX_SIZE),
            ..Default::default()
        });

        assert_eq!(dbg!(wm.get_wnd_size(&hwnd)), SIZE);
        let attrs = twm.wnd_attrs(&hwnd).unwrap();
        assert_eq!(attrs.caption, CAPTION);
        assert_eq!(attrs.visible, VIS);
        assert_eq!(attrs.size, SIZE);
        assert_eq!(attrs.min_size, MIN_SIZE);
        assert_eq!(attrs.max_size, MAX_SIZE);
        assert_eq!(attrs.flags, pal::WndFlags::default());

        wm.update_wnd(&hwnd);

        // TODO: change window size
        // TODO: change DPI scale
        // TODO: examine rendered contents

        wm.remove_wnd(&hwnd);

        assert!(twm.wnd_attrs(&hwnd).is_none());
    });
}

#[test]
fn plain_layer() {
    testing::run_test(|twm| {
        let wm = twm.wm();

        let hlayer = wm.new_layer(pal::LayerAttrs {
            ..Default::default()
        });
        wm.remove_layer(&hlayer);

        let hlayer = wm.new_layer(pal::LayerAttrs {
            bg_color: Some([0.2, 0.3, 0.4, 0.8].into()),
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            transform: Some(Matrix3::from_angle(Deg(30.0))),
            ..Default::default()
        });
        wm.remove_layer(&hlayer);
    });
}

#[test]
fn bitmap_layer() {
    testing::run_test(|twm| {
        let wm = twm.wm();

        let bmp = {
            let mut bmp_builder = pal::BitmapBuilder::new([100, 100]);
            bmp_builder.set_stroke_rgb([0.0, 0.0, 0.0, 1.0].into());
            bmp_builder.move_to(Point2::new(20.0, 20.0));
            bmp_builder.line_to(Point2::new(80.0, 20.0));
            bmp_builder.line_to(Point2::new(20.0, 80.0));
            bmp_builder.line_to(Point2::new(80.0, 80.0));
            bmp_builder.quad_bezier_to(Point2::new(80.0, 20.0), Point2::new(20.0, 20.0));
            bmp_builder.stroke();
            bmp_builder.into_bitmap()
        };

        let hlayer = wm.new_layer(pal::LayerAttrs {
            contents: Some(Some(bmp.clone())),
            ..Default::default()
        });
        wm.remove_layer(&hlayer);

        let hlayer = wm.new_layer(pal::LayerAttrs {
            contents: Some(Some(bmp.clone())),
            bg_color: Some([0.2, 0.3, 0.4, 0.8].into()),
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            contents_scale: Some(1.5),
            contents_center: Some(box2! {
                min: [0.3, 0.3], max: [0.8, 0.8],
            }),
            ..Default::default()
        });
        wm.remove_layer(&hlayer);
    });
}

#[test]
fn wnd_with_layer() {
    testing::run_test(|twm| {
        let wm = twm.wm();

        let hlayer = wm.new_layer(pal::LayerAttrs {
            bg_color: Some([0.2, 0.3, 0.4, 0.8].into()),
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            transform: Some(Matrix3::from_angle(Deg(30.0))),
            ..Default::default()
        });

        const SIZE: [u32; 2] = [100, 200];

        let hwnd = wm.new_wnd(pal::WndAttrs {
            caption: Some("Hello world".into()),
            visible: Some(true),
            size: Some(SIZE),
            min_size: Some([50, 100]),
            max_size: Some([300, 300]),
            layer: Some(Some(hlayer.clone())),
            ..Default::default()
        });

        wm.update_wnd(&hwnd);

        // TODO: change window size
        // TODO: change DPI scale

        wm.remove_wnd(&hwnd);
        wm.remove_layer(&hlayer);
    });
}
