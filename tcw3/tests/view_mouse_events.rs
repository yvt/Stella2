use cggeom::prelude::*;
use cgmath::Point2;
use std::{cell::RefCell, mem::replace, rc::Rc};
use try_match::try_match;

use tcw3::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{
        layouts::{EmptyLayout, FillLayout, TableLayout},
        AlignFlags,
    },
    uicore::{
        HView, HViewRef, HWnd, ScrollDelta, ScrollListener, SizeTraits, ViewFlags, ViewListener,
    },
};

#[derive(Debug, PartialEq)]
enum Event {
    MouseEnter,
    MouseLeave,
    MouseOver,
    MouseOut,
    ScrollMotion,
    ScrollGesture,
}

struct RecordingViewListener(u8, Rc<RefCell<Vec<(u8, Event)>>>);

impl ViewListener for RecordingViewListener {
    fn mouse_enter(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::MouseEnter));
    }
    fn mouse_leave(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::MouseLeave));
    }
    fn mouse_over(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::MouseOver));
    }
    fn mouse_out(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::MouseOut));
    }

    fn scroll_motion(&self, _: pal::Wm, _: HViewRef<'_>, _loc: Point2<f32>, _delta: &ScrollDelta) {
        self.1.borrow_mut().push((self.0, Event::ScrollMotion));
    }
    fn scroll_gesture(
        &self,
        _: pal::Wm,
        _: HViewRef<'_>,
        _loc: Point2<f32>,
    ) -> Box<dyn ScrollListener> {
        self.1.borrow_mut().push((self.0, Event::ScrollGesture));
        Box::new(())
    }
}

macro_rules! flush_and_assert_events {
    ($events:expr, $expected:expr) => {
        assert_eq!(replace(&mut *$events.borrow_mut(), Vec::new()), $expected);
    };
}

#[use_testing_wm]
#[test]
fn mouse_over_evts(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    let view0 = HView::new(ViewFlags::default());
    let view1 = HView::new(ViewFlags::ACCEPT_MOUSE_OVER);
    let view2 = HView::new(ViewFlags::ACCEPT_MOUSE_OVER);
    let view3 = HView::new(ViewFlags::ACCEPT_MOUSE_OVER);
    let view4 = HView::new(ViewFlags::ACCEPT_MOUSE_OVER);

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));
    view2.set_listener(RecordingViewListener(2, events.clone()));
    view3.set_listener(RecordingViewListener(3, events.clone()));
    view4.set_listener(RecordingViewListener(4, events.clone()));

    view1.set_layout(FillLayout::new(view2.clone()).with_uniform_margin(10.0));
    view2.set_layout(EmptyLayout::new(
        SizeTraits::default().with_preferred([20.0; 2].into()),
    ));
    view3.set_layout(FillLayout::new(view4.clone()).with_uniform_margin(10.0));
    view4.set_layout(EmptyLayout::new(
        SizeTraits::default().with_preferred([20.0; 2].into()),
    ));

    view0.set_layout(
        TableLayout::stack_horz(vec![
            (view1.clone(), AlignFlags::JUSTIFY),
            (view3.clone(), AlignFlags::JUSTIFY),
        ])
        .with_uniform_margin(10.0),
    );

    wnd.content_view().set_layout(FillLayout::new(view0));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    flush_and_assert_events!(events, []);

    // `view0` does not have `ACCEPT_MOUSE_OVER`, so moving the mouse
    // pointer over it does not cause `mouse_over`
    twm.raise_mouse_motion(&pal_hwnd, [0.0; 2].into());
    flush_and_assert_events!(events, []);

    // The mouse pointer is on `view2`, which is a child of `view1`.
    // `view0` receives `mouse_enter` because of its subview receiving
    // `mouse_over`.
    twm.raise_mouse_motion(&pal_hwnd, view2.global_frame().mid());
    flush_and_assert_events!(
        events,
        [
            (0, Event::MouseEnter),
            (1, Event::MouseEnter),
            (2, Event::MouseEnter),
            (2, Event::MouseOver),
        ]
    );

    // The mouse pointer is on `view4`, which is a child of `view3`
    twm.raise_mouse_motion(&pal_hwnd, view4.global_frame().mid());
    flush_and_assert_events!(
        events,
        [
            (2u8, Event::MouseOut),
            (2, Event::MouseLeave),
            (1, Event::MouseLeave),
            (3, Event::MouseEnter),
            (4, Event::MouseEnter),
            (4, Event::MouseOver),
        ]
    );

    // The mouse pointer is on `view3`
    twm.raise_mouse_motion(&pal_hwnd, view3.global_frame().min);
    flush_and_assert_events!(
        events,
        [
            (4u8, Event::MouseOut),
            (4, Event::MouseLeave),
            (3, Event::MouseOver),
        ]
    );

    // No hot view
    twm.raise_mouse_motion(&pal_hwnd, [0.0; 2].into());
    flush_and_assert_events!(
        events,
        [
            (3, Event::MouseOut),
            (3, Event::MouseLeave),
            (0, Event::MouseLeave),
        ]
    );

    twm.raise_mouse_leave(&pal_hwnd);
    flush_and_assert_events!(events, []);
}

#[use_testing_wm]
#[test]
fn scroll_evts(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    let view0 = HView::new(ViewFlags::default());
    let view1 = HView::new(ViewFlags::ACCEPT_SCROLL);
    let view2 = HView::new(ViewFlags::default());

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));
    view2.set_listener(RecordingViewListener(2, events.clone()));

    view0.set_layout(FillLayout::new(view1.clone()).with_uniform_margin(10.0));

    view1.set_layout(FillLayout::new(view2.clone()).with_uniform_margin(10.0));

    view2.set_layout(EmptyLayout::new(
        SizeTraits::default().with_preferred([20.0; 2].into()),
    ));

    wnd.content_view().set_layout(FillLayout::new(view0));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    flush_and_assert_events!(events, []);

    let delta = ScrollDelta {
        delta: [5.0; 2].into(),
        precise: true,
    };

    // `view0` does not have `ACCEPT_SCROLL`, so moving the mouse wheel with
    // the mouse pointer over it does not cause `scroll_motion`
    twm.raise_scroll_motion(&pal_hwnd, [0.0; 2].into(), &delta);
    let g = twm.raise_scroll_gesture(&pal_hwnd, [0.0; 2].into());
    g.end();
    drop(g);
    flush_and_assert_events!(events, []);

    // `view1`, on the other hand
    twm.raise_scroll_motion(&pal_hwnd, [30.0; 2].into(), &delta);
    flush_and_assert_events!(events, [(1, Event::ScrollMotion)]);

    let g = twm.raise_scroll_gesture(&pal_hwnd, [30.0; 2].into());
    g.end();
    drop(g);
    flush_and_assert_events!(events, [(1, Event::ScrollGesture)]);
}
