use cggeom::prelude::*;
use cgmath::{Deg, Matrix3};
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
