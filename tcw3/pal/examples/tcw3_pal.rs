use cggeom::{box2, prelude::*};
use cgmath::{Point2, Vector2};
use log::info;
use std::convert::TryFrom;
use tcw3_pal::{self as pal, prelude::*};

static KNOWN_KEYS: pal::AccelTable = pal::accel_table![
    ('A' as _, windows("A"), macos("A"), gtk("A")),
    ('B' as _, windows("B"), macos("B"), gtk("B")),
    ('C' as _, windows("C"), macos("C"), gtk("C")),
    ('D' as _, windows("D"), macos("D"), gtk("D")),
    ('E' as _, windows("E"), macos("E"), gtk("E")),
    ('F' as _, windows("F"), macos("F"), gtk("F")),
    ('G' as _, windows("G"), macos("G"), gtk("G")),
    ('H' as _, windows("H"), macos("H"), gtk("H")),
    ('I' as _, windows("I"), macos("I"), gtk("I")),
    ('J' as _, windows("J"), macos("J"), gtk("J")),
    ('K' as _, windows("K"), macos("K"), gtk("K")),
    ('L' as _, windows("L"), macos("L"), gtk("L")),
    ('M' as _, windows("M"), macos("M"), gtk("M")),
    ('N' as _, windows("N"), macos("N"), gtk("N")),
    ('O' as _, windows("O"), macos("O"), gtk("O")),
    ('P' as _, windows("P"), macos("P"), gtk("P")),
    ('Q' as _, windows("Q"), macos("Q"), gtk("Q")),
    ('R' as _, windows("R"), macos("R"), gtk("R")),
    ('S' as _, windows("S"), macos("S"), gtk("S")),
    ('T' as _, windows("T"), macos("T"), gtk("T")),
    ('U' as _, windows("U"), macos("U"), gtk("U")),
    ('V' as _, windows("V"), macos("V"), gtk("V")),
    ('W' as _, windows("W"), macos("W"), gtk("W")),
    ('X' as _, windows("X"), macos("X"), gtk("X")),
    ('Y' as _, windows("Y"), macos("Y"), gtk("Y")),
    ('Z' as _, windows("Z"), macos("Z"), gtk("Z")),
    ('↑' as _, windows("Up"), macos("Up"), gtk("Up")),
    ('↓' as _, windows("Down"), macos("Down"), gtk("Down")),
    ('←' as _, windows("Left"), macos("Left"), gtk("Left")),
    ('→' as _, windows("Right"), macos("Right"), gtk("Right")),
];

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

    fn focus(&self, wm: pal::Wm, wnd: &pal::HWnd) {
        info!("is_wnd_focused = {:?}", wm.is_wnd_focused(wnd));
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

    fn key_down(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        e: &dyn pal::iface::KeyEvent<pal::AccelTable>,
    ) -> bool {
        if let Some(code) = e.translate_accel(&KNOWN_KEYS) {
            info!("key_down({:?})", char::try_from(code as u32).unwrap());
            true
        } else {
            false
        }
    }

    fn key_up(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        e: &dyn pal::iface::KeyEvent<pal::AccelTable>,
    ) -> bool {
        if let Some(code) = e.translate_accel(&KNOWN_KEYS) {
            info!("key_up({:?})", char::try_from(code as u32).unwrap());
            true
        } else {
            false
        }
    }

    fn interpret_event(
        &self,
        _: pal::Wm,
        _: &pal::HWnd,
        ctx: &mut dyn pal::iface::InterpretEventCtx<pal::AccelTable>,
    ) {
        ctx.use_accel(&pal::accel_table![
            (
                pal::actions::COPY,
                windows("Ctrl+C"),
                gtk("Ctrl+C"),
                macos_sel("copy:")
            ),
            (
                1,
                windows("Q"),
                macos("Q"),
                gtk("Q"),
                macos_sel("terminate:")
            ),
            (
                2,
                windows("Ctrl+O"),
                gtk("Ctrl+O"),
                macos_sel("openDocument:")
            )
        ]);
    }

    fn validate_action(&self, _: pal::Wm, _: &pal::HWnd, _: pal::ActionId) -> pal::ActionStatus {
        pal::ActionStatus::VALID | pal::ActionStatus::ENABLED
    }

    fn perform_action(&self, wm: pal::Wm, _: &pal::HWnd, action: pal::ActionId) {
        info!("perform_action({:?})", action);
        if action == 1 {
            wm.terminate();
            return;
        }
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

    // Creating an application menu is out of the scope of TCW3, but TCW3
    // can still route the messages generated by menu items.
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::{
            appkit::{NSApplication, NSMenu, NSMenuItem},
            base::nil,
            foundation::{NSAutoreleasePool, NSString},
        };
        use objc::runtime::Sel;
        let menu = NSMenu::alloc(nil)
            .autorelease()
            .initWithTitle_(NSString::alloc(nil).autorelease().init_str(""));

        {
            let appname = menu.addItemWithTitle_action_keyEquivalent(
                NSString::alloc(nil)
                    .autorelease()
                    .init_str("TCW3 PAL Test Application"),
                Sel::from_ptr(std::ptr::null()),
                NSString::alloc(nil).autorelease().init_str(""),
            );

            let appname_menu = NSMenu::new(nil);
            appname.setSubmenu_(appname_menu);

            appname_menu.addItemWithTitle_action_keyEquivalent(
                NSString::alloc(nil).autorelease().init_str("Quit tcw3_pal"),
                Sel::register("terminate:"),
                NSString::alloc(nil).autorelease().init_str("q"),
            );
        }

        {
            let file = NSMenuItem::alloc(nil).autorelease();

            let file_menu = NSMenu::alloc(nil)
                .autorelease()
                .initWithTitle_(NSString::alloc(nil).autorelease().init_str("File"));
            file.setSubmenu_(file_menu);

            menu.addItem_(file);

            file_menu.addItemWithTitle_action_keyEquivalent(
                NSString::alloc(nil).init_str("Open"),
                Sel::register("openDocument:"),
                NSString::alloc(nil).init_str("o"),
            );
        }

        {
            let edit = NSMenuItem::alloc(nil).autorelease();

            let edit_menu = NSMenu::alloc(nil)
                .autorelease()
                .initWithTitle_(NSString::alloc(nil).autorelease().init_str("Edit"));
            edit.setSubmenu_(edit_menu);

            menu.addItem_(edit);

            edit_menu.addItemWithTitle_action_keyEquivalent(
                NSString::alloc(nil).init_str("Cut"),
                Sel::register("cut:"),
                NSString::alloc(nil).init_str("x"),
            );
            edit_menu.addItemWithTitle_action_keyEquivalent(
                NSString::alloc(nil).init_str("Copy"),
                Sel::register("copy:"),
                NSString::alloc(nil).init_str("c"),
            );
            edit_menu.addItemWithTitle_action_keyEquivalent(
                NSString::alloc(nil).init_str("Paste"),
                Sel::register("paste:"),
                NSString::alloc(nil).init_str("v"),
            );
        }

        let app = cocoa::appkit::NSApp();
        app.setMainMenu_(menu);
    }

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
