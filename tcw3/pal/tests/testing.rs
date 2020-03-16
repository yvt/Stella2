use cggeom::{box2, prelude::*, Box2};
use cgmath::{Deg, Matrix3, Point2, Vector2};
use log::info;
use std::{
    cell::Cell,
    ops::Range,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::spawn,
    time::{Duration, Instant},
};
use tcw3_pal::{self as pal, iface::Wm as _, prelude::*, testing, testing::wmapi, MtLock, Wm};

fn init_logger() {
    // Copied from `tcw3/tesing/src/lib.rs`, which can't be imported from here
    // because of a circular dependency
    let inner = env_logger::builder().is_test(true).build();
    let max_level = inner.filter();
    if testing::Logger::new(Box::new(inner)).try_init().is_ok() {
        log::set_max_level(max_level);
    }
}

#[test]
fn create_testing_wm() {
    init_logger();
    testing::run_test(|twm| {
        // This block might or might not run depending on a feature flag
        twm.step_until(Instant::now() + Duration::from_millis(100));
    });
}

#[test]
fn invoke() {
    init_logger();
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
fn invoke_eradication() {
    init_logger();
    let obj = Arc::new(());

    let obj2 = Arc::clone(&obj);
    testing::run_test(move |twm| {
        twm.wm().invoke(move |_| {
            let _ = obj2;
        });

        // Don't `step`, just return. The closure should be dropped by
        // `eradicate_events` automatically after we return the control
    });

    testing::run_test(move |_| {
        assert_eq!(Arc::strong_count(&obj), 1);
    });
}

#[test]
fn invoke_during_eradication() {
    init_logger();
    let flag = Arc::new(AtomicBool::new(false));

    let flag2 = Arc::clone(&flag);
    testing::run_test(move |twm| {
        struct SetOnDrop(pal::Wm, Arc<AtomicBool>);
        impl Drop for SetOnDrop {
            fn drop(&mut self) {
                let flag = Arc::clone(&self.1);
                flag.store(true, Ordering::Relaxed);

                // `invoke` is allowed here (shouldn't panic), though the
                // closure might not actually be called
                self.0.invoke(move |_| {});
            }
        }

        let sod = SetOnDrop(twm.wm(), flag2);
        twm.wm().invoke(move |_| {
            let _ = sod;
        });

        // Don't `step`, just return. The closure should be dropped by
        // `eradicate_events` automatically after we return the control
        // When the closure is dropped, `SetOnDrop::drop` is called to
        // set the flag.
    });

    testing::run_test(move |wm| {
        wm.step_unsend();
        assert!(flag.load(Ordering::Relaxed));
    });
}

#[test]
fn invoke_after_eradication() {
    init_logger();
    let obj = Arc::new(());

    let obj2 = Arc::clone(&obj);
    testing::run_test(move |twm| {
        let d_600_ms = Duration::from_millis(600);

        twm.wm().invoke_after(d_600_ms..d_600_ms, move |_| {
            let _ = obj2;
            unreachable!();
        });

        // Don't `step`, just return. The closure should be dropped by
        // `eradicate_events` automatically after we return the control
    });

    testing::run_test(move |_| {
        assert_eq!(Arc::strong_count(&obj), 1);
    });
}

#[test]
fn step_unsend() {
    init_logger();
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
    init_logger();
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
fn invoke_after() {
    init_logger();
    testing::run_test(|twm| {
        let d_200_ms = Duration::from_millis(200);
        let d_600_ms = Duration::from_millis(600);
        let d_1200_ms = Duration::from_millis(1200);

        let flag = Rc::new(Cell::new(false));
        {
            let flag = Rc::clone(&flag);
            twm.wm()
                .invoke_after(d_600_ms..d_1200_ms, move |_| flag.set(true));
        }

        // The closure shouldn't be called too soon
        assert!(!flag.get());
        twm.step_until(Instant::now() + d_200_ms);
        assert!(!flag.get());

        // Wait until the closure is called and the flag is set
        while !flag.get() {
            twm.step_until(Instant::now() + d_200_ms);
        }
    });
}

#[test]
fn invoke_after_cancel() {
    init_logger();
    testing::run_test(|twm| {
        let d_200_ms = Duration::from_millis(200);
        let d_600_ms = Duration::from_millis(600);

        let flag = Rc::new(Cell::new(false));
        let hinvoke = {
            let flag = Rc::clone(&flag);
            twm.wm().invoke_after(d_600_ms..d_600_ms, move |_| {
                flag.set(true);
            })
        };

        // The closure shouldn't be called too soon
        assert!(Rc::strong_count(&flag) > 1);
        twm.step_until(Instant::now() + d_200_ms);

        // Cancel the invocation
        twm.wm().cancel_invoke(&hinvoke);

        // The closure should be dropped without being called
        assert!(!flag.get());
        assert!(Rc::strong_count(&flag) == 1);
    });
}

#[test]
#[should_panic]
fn panicking() {
    let flag = Arc::new(AtomicBool::new(false));

    let flag2 = Arc::clone(&flag);
    testing::run_test(move |_| {
        flag2.store(true, Ordering::Relaxed);
        panic!("this panic should be contained to this test case");
    });

    if !flag.load(Ordering::Relaxed) {
        // The closure did not run because `testing` backend is disabled.
        // Cause a panic here to prevent the false test failure.
        panic!("skipping this test because `testing` is apparently disabled");
    }
}

#[test]
fn bitmap_size() {
    init_logger();
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
    init_logger();
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
    init_logger();
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
    init_logger();
    testing::run_test(|_| {
        let mut b = pal::BitmapBuilder::new([12, 34]);

        let style = pal::CharStyle::new(Default::default());

        let layout = pal::TextLayout::from_text("20% cooler", &style, None);

        info!("layout_bounds = {:?}", layout.layout_bounds());
        info!("visual_bounds = {:?}", layout.visual_bounds());

        b.draw_text(&layout, [10.0, 10.0].into(), [0.3, 0.5, 0.6, 1.0].into());
    });
}

#[test]
fn run_test_reset() {
    init_logger();
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
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        const CAPTION: &str = "Hello world";
        const VIS: bool = true;
        const SIZE: [u32; 2] = [100, 200];
        const MIN_SIZE: [u32; 2] = [50, 100];
        const MAX_SIZE: [u32; 2] = [300, 300];
        const CURSOR: pal::CursorShape = pal::CursorShape::Hand;

        let hwnd = wm.new_wnd(pal::WndAttrs {
            caption: Some(CAPTION.into()),
            visible: Some(VIS),
            size: Some(SIZE),
            min_size: Some(MIN_SIZE),
            max_size: Some(MAX_SIZE),
            cursor_shape: Some(CURSOR),
            ..Default::default()
        });

        assert_eq!(twm.hwnds().len(), 1);

        info!("get_wnd_size({:?}) = {:?}", hwnd, wm.get_wnd_size(&hwnd));
        assert_eq!(wm.get_wnd_size(&hwnd), SIZE);
        let attrs = twm.wnd_attrs(&hwnd).unwrap();
        assert_eq!(attrs.caption, CAPTION);
        assert_eq!(attrs.visible, VIS);
        assert_eq!(attrs.size, SIZE);
        assert_eq!(attrs.min_size, MIN_SIZE);
        assert_eq!(attrs.max_size, MAX_SIZE);
        assert_eq!(attrs.cursor_shape, CURSOR);
        assert_eq!(attrs.flags, pal::WndFlags::default());

        wm.update_wnd(&hwnd);

        let mut ss = wmapi::WndSnapshot::new();

        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);

        assert_eq!(ss.size, [SIZE[0] as usize, SIZE[1] as usize]);
        assert!(ss.stride >= ss.size[0] * 4);

        twm.set_wnd_size(&hwnd, [50, 200]);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);

        twm.set_wnd_dpi_scale(&hwnd, 1.5);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);

        wm.remove_wnd(&hwnd);

        assert!(twm.wnd_attrs(&hwnd).is_none());
    });
}

fn snapshot_find_nontransparent_pixel(
    wmapi::WndSnapshot { size, data, stride }: &wmapi::WndSnapshot,
) -> Option<[usize; 2]> {
    for y in 0..size[1] {
        let row = &data[y * stride..];

        for (x, pixel) in row[..size[0] * 4].chunks_exact(4).enumerate() {
            if pixel[3] != 0 {
                return Some([x, y]);
            }
        }
    }

    None
}

fn assert_snapshot_empty(ss: &wmapi::WndSnapshot) {
    if let Some(p) = snapshot_find_nontransparent_pixel(ss) {
        panic!("Found a non-transparent pixel at {:?}", p);
    }
}

fn assert_snapshot_nonempty(ss: &wmapi::WndSnapshot) {
    snapshot_find_nontransparent_pixel(ss).expect("Did not find a non-transparent pixel");
}

#[test]
fn plain_layer() {
    init_logger();
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
    init_logger();
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
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        let hlayer = wm.new_layer(pal::LayerAttrs {
            bg_color: Some([0.2, 0.3, 0.4, 0.8].into()),
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            transform: Some(Matrix3::from_angle(Deg(30.0))),
            bounds: Some(box2! { top_left: [10.0; 2], size: [30.0; 2] }),
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

        let mut ss = wmapi::WndSnapshot::new();

        wm.update_wnd(&hwnd);

        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);

        twm.set_wnd_size(&hwnd, [200, 200]);

        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);

        wm.update_wnd(&hwnd);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);

        twm.set_wnd_dpi_scale(&hwnd, 2.0);

        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);

        wm.remove_wnd(&hwnd);
        wm.remove_layer(&hlayer);
    });
}

#[test]
fn defer_layer_changes_until_update_wnd() {
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        let hlayer = wm.new_layer(pal::LayerAttrs {
            bg_color: Some([0.2, 0.3, 0.4, 0.8].into()),
            flags: Some(pal::LayerFlags::MASK_TO_BOUNDS),
            transform: Some(Matrix3::from_angle(Deg(30.0))),
            // Off-screen
            bounds: Some(box2! { top_left: [-40.0; 2], size: [30.0; 2] }),
            ..Default::default()
        });

        let hwnd = wm.new_wnd(pal::WndAttrs {
            visible: Some(true),
            size: Some([100, 100]),
            layer: Some(Some(hlayer.clone())),
            ..Default::default()
        });

        wm.update_wnd(&hwnd);

        let mut ss = wmapi::WndSnapshot::new();

        // The layer is off-screen
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);

        // Move the layer to inside the window
        wm.set_layer_attr(
            &hlayer,
            pal::LayerAttrs {
                bounds: Some(box2! { top_left: [10.0; 2], size: [30.0; 2] }),
                ..Default::default()
            },
        );

        // The layer is still off-screen since we haven't called `update_wnd` yet
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_empty(&ss);

        wm.update_wnd(&hwnd);

        // Now the layer is visible
        twm.read_wnd_snapshot(&hwnd, &mut ss);
        assert_snapshot_nonempty(&ss);

        wm.remove_wnd(&hwnd);
        wm.remove_layer(&hlayer);
    });
}

#[test]
fn wnd_close_event() {
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        #[derive(Clone)]
        struct Listener(Rc<Cell<u8>>);
        impl WndListener<pal::Wm> for Listener {
            fn close_requested(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 0);
                self.0.set(1);
            }
        }

        let state = Rc::new(Cell::new(0));

        let hwnd = wm.new_wnd(pal::WndAttrs {
            visible: Some(true),
            listener: Some(Box::new(Listener(Rc::clone(&state)))),
            ..Default::default()
        });

        assert_eq!(state.get(), 0);
        twm.raise_close_requested(&hwnd);
        assert_eq!(state.get(), 1);
    });
}

#[test]
fn wnd_size_events() {
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        #[derive(Clone)]
        struct Listener(Rc<Cell<u8>>);
        impl WndListener<pal::Wm> for Listener {
            fn resize(&self, wm: pal::Wm, hwnd: &pal::HWnd) {
                assert_eq!(self.0.get(), 0);
                self.0.set(1);

                // Should match the value given to `set_wnd_size`. It shouldn't
                // be clipped by `max_size`.
                assert_eq!(wm.get_wnd_size(hwnd), [200; 2]);
            }
            fn dpi_scale_changed(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 1);
                self.0.set(2);
            }
        }

        let state = Rc::new(Cell::new(0));

        let hwnd = wm.new_wnd(pal::WndAttrs {
            visible: Some(true),
            size: Some([100; 2]),
            max_size: Some([150; 2]),
            listener: Some(Box::new(Listener(Rc::clone(&state)))),
            ..Default::default()
        });

        assert_eq!(state.get(), 0);
        twm.set_wnd_size(&hwnd, [200; 2]);
        assert_eq!(state.get(), 1);
        twm.set_wnd_dpi_scale(&hwnd, 2.0);
        assert_eq!(state.get(), 2);
    });
}

#[test]
fn wnd_mouse_events() {
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        #[derive(Clone)]
        struct Listener(Rc<Cell<u8>>);
        impl WndListener<pal::Wm> for Listener {
            fn mouse_motion(&self, _: pal::Wm, _: &pal::HWnd, _loc: Point2<f32>) {
                assert_eq!(self.0.get(), 1);
                self.0.set(2);
            }

            fn mouse_leave(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 0);
                self.0.set(1);
            }

            fn mouse_drag(
                &self,
                _: pal::Wm,
                _: &pal::HWnd,
                _loc: Point2<f32>,
                _button: u8,
            ) -> Box<dyn MouseDragListener<pal::Wm>> {
                assert_eq!(self.0.get(), 2);
                self.0.set(3);
                Box::new(self.clone())
            }

            fn scroll_motion(
                &self,
                _: pal::Wm,
                _: &pal::HWnd,
                _loc: Point2<f32>,
                _delta: &pal::ScrollDelta,
            ) {
                assert_eq!(self.0.get(), 7);
                self.0.set(8);
            }

            fn scroll_gesture(
                &self,
                _: pal::Wm,
                _: &pal::HWnd,
                _loc: Point2<f32>,
            ) -> Box<dyn ScrollListener<pal::Wm>> {
                assert_eq!(self.0.get(), 8);
                self.0.set(9);
                Box::new(self.clone())
            }
        }

        impl MouseDragListener<pal::Wm> for Listener {
            fn mouse_motion(&self, _: pal::Wm, _: &pal::HWnd, _loc: Point2<f32>) {
                assert_eq!(self.0.get(), 4);
                self.0.set(5);
            }
            fn mouse_down(&self, _: pal::Wm, _: &pal::HWnd, _loc: Point2<f32>, _button: u8) {
                assert_eq!(self.0.get(), 3);
                self.0.set(4);
            }
            fn mouse_up(&self, _: pal::Wm, _: &pal::HWnd, _loc: Point2<f32>, _button: u8) {
                assert_eq!(self.0.get(), 5);
                self.0.set(6);
            }
            fn cancel(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 4);
                self.0.set(7);
            }
        }

        impl ScrollListener<pal::Wm> for Listener {
            fn motion(
                &self,
                _: pal::Wm,
                _: &pal::HWnd,
                _delta: &pal::ScrollDelta,
                _velocity: Vector2<f32>,
            ) {
                assert_eq!(self.0.get(), 9);
                self.0.set(10);
            }
            fn start_momentum_phase(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 10);
                self.0.set(11);
            }
            fn end(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 11);
                self.0.set(12);
            }
            fn cancel(&self, _: pal::Wm, _: &pal::HWnd) {
                assert_eq!(self.0.get(), 12);
                self.0.set(13);
            }
        }

        let state = Rc::new(Cell::new(0));

        let hwnd = wm.new_wnd(pal::WndAttrs {
            visible: Some(true),
            size: Some([100; 2]),
            listener: Some(Box::new(Listener(Rc::clone(&state)))),
            ..Default::default()
        });

        assert_eq!(state.get(), 0);
        twm.raise_mouse_leave(&hwnd);
        assert_eq!(state.get(), 1);
        twm.raise_mouse_motion(&hwnd, [20.0; 2].into());
        assert_eq!(state.get(), 2);
        let drag = twm.raise_mouse_drag(&hwnd, [20.0; 2].into(), 0);
        assert_eq!(state.get(), 3);
        drag.mouse_down([20.0; 2].into(), 0);
        assert_eq!(state.get(), 4);
        drag.mouse_motion([30.0; 2].into());
        assert_eq!(state.get(), 5);
        drag.mouse_up([20.0; 2].into(), 0);
        assert_eq!(state.get(), 6);

        state.set(3);
        drag.mouse_down([20.0; 2].into(), 1);
        assert_eq!(state.get(), 4);
        drag.cancel();
        assert_eq!(state.get(), 7);

        twm.raise_scroll_motion(
            &hwnd,
            [20.0; 2].into(),
            &pal::ScrollDelta {
                precise: false,
                delta: [10.0; 2].into(),
            },
        );
        assert_eq!(state.get(), 8);
        drop(drag);

        let scroll = twm.raise_scroll_gesture(&hwnd, [20.0; 2].into());
        assert_eq!(state.get(), 9);

        scroll.motion(
            &pal::ScrollDelta {
                precise: true,
                delta: [10.0; 2].into(),
            },
            [0.0; 2].into(),
        );
        assert_eq!(state.get(), 10);
        scroll.start_momentum_phase();
        assert_eq!(state.get(), 11);
        scroll.end();
        assert_eq!(state.get(), 12);
        drop(scroll);

        state.set(8);
        let scroll = twm.raise_scroll_gesture(&hwnd, [20.0; 2].into());
        state.set(12);
        scroll.cancel();
        assert_eq!(state.get(), 13);
        drop(scroll);
    });
}

#[test]
fn wnd_focus_event() {
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        #[derive(Clone)]
        struct Listener(Rc<Cell<u8>>);
        impl WndListener<pal::Wm> for Listener {
            fn focus(&self, wm: pal::Wm, hwnd: &pal::HWnd) {
                assert_eq!(self.0.get(), wm.is_wnd_focused(hwnd) as u8);
                self.0.set(self.0.get() + 2);
            }
        }

        let state = Rc::new(Cell::new(1));

        let hwnd = wm.new_wnd(pal::WndAttrs {
            visible: Some(true),
            listener: Some(Box::new(Listener(Rc::clone(&state)))),
            ..Default::default()
        });

        twm.set_wnd_focused(&hwnd, true);
        assert_eq!(state.get(), 3);

        state.set(0);
        twm.set_wnd_focused(&hwnd, false);
        assert_eq!(state.get(), 2);
    });
}

#[test]
fn text_input_ctx() {
    init_logger();
    testing::run_test(|twm| {
        let wm = twm.wm();

        #[derive(Clone)]
        struct Listener(Rc<Cell<u8>>);
        impl TextInputCtxListener<pal::Wm> for Listener {
            fn edit(
                &self,
                _wm: pal::Wm,
                _: &pal::HTextInputCtx,
                _mutating: bool,
            ) -> Box<dyn TextInputCtxEdit<pal::Wm> + '_> {
                Box::new(self.clone())
            }
        }

        impl TextInputCtxEdit<pal::Wm> for Listener {
            fn selected_range(&mut self) -> Range<usize> {
                unreachable!()
            }
            fn set_selected_range(&mut self, _range: Range<usize>) {
                assert_eq!(self.0.get(), 1);
                self.0.set(2);
            }
            fn set_composition_range(&mut self, _range: Option<Range<usize>>) {
                unreachable!()
            }
            fn replace(&mut self, _range: Range<usize>, _text: &str) {
                unreachable!()
            }
            fn slice(&mut self, _range: Range<usize>) -> String {
                unreachable!()
            }
            fn floor_index(&mut self, _i: usize) -> usize {
                unreachable!()
            }
            fn ceil_index(&mut self, _i: usize) -> usize {
                unreachable!()
            }
            fn len(&mut self) -> usize {
                unreachable!()
            }
            fn index_from_point(
                &mut self,
                _point: Point2<f32>,
                _flags: pal::IndexFromPointFlags,
            ) -> Option<usize> {
                unreachable!()
            }
            fn frame(&mut self) -> Box2<f32> {
                unreachable!()
            }
            fn slice_bounds(&mut self, _range: Range<usize>) -> (Box2<f32>, usize) {
                unreachable!()
            }
        }

        let state = Rc::new(Cell::new(1));

        let hwnd = wm.new_wnd(pal::WndAttrs {
            visible: Some(true),
            ..Default::default()
        });

        let tictx = wm.new_text_input_ctx(&hwnd, Box::new(Listener(Rc::clone(&state))));

        // Test the edit event forwarding
        {
            let mut edit = twm.raise_edit(&tictx, false);
            edit.set_selected_range(0..4);
        }
        assert_eq!(state.get(), 2);

        // Test the active context management
        assert!(twm.expect_unique_active_text_input_ctx().is_none());
        wm.text_input_ctx_set_active(&tictx, true);
        assert_eq!(
            twm.expect_unique_active_text_input_ctx().as_ref(),
            Some(&tictx)
        );
        wm.text_input_ctx_set_active(&tictx, false);
        assert!(twm.expect_unique_active_text_input_ctx().is_none());
    });
}
