use cggeom::prelude::*;
use std::{cell::RefCell, mem::replace, rc::Rc};
use try_match::try_match;

use tcw3::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{
        layouts::{EmptyLayout, TableLayout},
        AlignFlags,
    },
    uicore::{HView, HViewRef, HWnd, SizeTraits, ViewFlags, ViewListener},
};

#[derive(Debug, PartialEq)]
enum Event {
    FocusEnter,
    FocusLeave,
    FocusGot,
    FocusLost,
}

struct RecordingViewListener(u8, Rc<RefCell<Vec<(u8, Event)>>>);

impl ViewListener for RecordingViewListener {
    fn focus_enter(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::FocusEnter));
    }
    fn focus_leave(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::FocusLeave));
    }
    fn focus_got(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::FocusGot));
    }
    fn focus_lost(&self, _: pal::Wm, _: HViewRef<'_>) {
        self.1.borrow_mut().push((self.0, Event::FocusLost));
    }
}

fn new_layout(views: impl IntoIterator<Item = HView>) -> TableLayout {
    TableLayout::stack_horz(views.into_iter().map(|v| (v, AlignFlags::JUSTIFY)))
}

macro_rules! new_view_tree {
    {
        let $view:ident = $init:expr;
        $({
            $(
                let $child:ident = $child_init:expr; $({ $($grandchildren:tt)* })?
            )*
        })?
    } => {
        $($( new_view_tree! { let $child = $child_init; $({ $($grandchildren)* })? } )*)?
        let $view = $init;
        $view.set_layout(new_layout(vec![
            $($( $child.clone() ),*)?
        ]));
    };
}

#[use_testing_wm]
#[test]
fn focus_evts(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    macro_rules! flush_and_assert_events {
        ($expected:expr) => {
            twm.step_unsend();
            assert_eq!(replace(&mut *events.borrow_mut(), Vec::new()), $expected);
        };
    }

    new_view_tree! {
        let view0 = HView::new(ViewFlags::default());
        {
            let view1 = HView::new(ViewFlags::TAB_STOP);
            {
                let view2 = HView::new(ViewFlags::TAB_STOP);
                {
                    let view5 = HView::new(ViewFlags::default());
                }
            }

            let view3 = HView::new(ViewFlags::TAB_STOP);
            {
                let view4 = HView::new(ViewFlags::TAB_STOP);
            }
        }
    }

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));
    view2.set_listener(RecordingViewListener(2, events.clone()));
    view3.set_listener(RecordingViewListener(3, events.clone()));
    view4.set_listener(RecordingViewListener(4, events.clone()));
    view5.set_listener(RecordingViewListener(5, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    flush_and_assert_events!([]);

    // `view0` does not have `TAB_STOP`. The first view having `TAB_STOP` in the
    // tab order is `view1`.
    view0.focus();
    flush_and_assert_events!([
        (0, Event::FocusEnter),
        (1, Event::FocusEnter),
        (1, Event::FocusGot),
    ]);

    // `view5` doesn't have `TAB_STOP`, so `view2` instead receives a keyboard
    // focus. `view2` has a keyboard focus, which is a child of `view1`.
    // `view0` receives `focus_enter` because of its subview receiving
    // `focus_enter`.
    view5.focus();
    flush_and_assert_events!([
        (1, Event::FocusLost),
        (2, Event::FocusEnter),
        (2, Event::FocusGot),
    ]);

    // The focus is on `view4`, which is a child of `view3`
    view4.focus();
    flush_and_assert_events!([
        (2u8, Event::FocusLost),
        (2, Event::FocusLeave),
        (1, Event::FocusLeave),
        (3, Event::FocusEnter),
        (4, Event::FocusEnter),
        (4, Event::FocusGot),
    ]);

    // The focus is on `view3`
    view3.focus();
    flush_and_assert_events!([
        (4u8, Event::FocusLost),
        (4, Event::FocusLeave),
        (3, Event::FocusGot),
    ]);

    // No focused view
    wnd.set_focused_view(None);
    flush_and_assert_events!([
        (3, Event::FocusLost),
        (3, Event::FocusLeave),
        (0, Event::FocusLeave),
    ]);

    wnd.set_focused_view(None);
    flush_and_assert_events!([]);
}

#[use_testing_wm]
#[test]
fn is_focused(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    new_view_tree! {
        let view0 = HView::new(ViewFlags::TAB_STOP);
        {
            let view1 = HView::new(ViewFlags::TAB_STOP);
        }
    }

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    assert_eq!([view0.is_focused(), view1.is_focused()], [false, false]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [false, false]
    );

    view0.focus();
    twm.step_unsend();

    assert_eq!([view0.is_focused(), view1.is_focused()], [true, false]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [true, false]
    );

    view1.focus();
    twm.step_unsend();

    assert_eq!([view0.is_focused(), view1.is_focused()], [false, true]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [true, true]
    );
}

#[use_testing_wm]
#[test]
fn view_removal(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    macro_rules! flush_and_assert_events {
        ($expected:expr) => {
            twm.step_unsend();
            assert_eq!(replace(&mut *events.borrow_mut(), Vec::new()), $expected);
        };
    }

    new_view_tree! {
        let view0 = HView::new(ViewFlags::default());
        {
            let view1 = HView::new(ViewFlags::TAB_STOP);
        }
    }

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    flush_and_assert_events!([]);

    view1.focus();
    flush_and_assert_events!([
        (0, Event::FocusEnter),
        (1, Event::FocusEnter),
        (1, Event::FocusGot),
    ]);

    // Remove the views from the window
    wnd.content_view().set_layout(new_layout(None));

    // Should not generate any focus events
    flush_and_assert_events!([]);

    // `is_focused` should return `false`
    assert_eq!([view0.is_focused(), view1.is_focused()], [false, false]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [false, false]
    );

    // `focused_view` should return `None`
    assert_eq!(wnd.focused_view(), None);
}

#[use_testing_wm]
#[test]
fn clear_tab_stop(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    macro_rules! flush_and_assert_events {
        ($expected:expr) => {
            twm.step_unsend();
            assert_eq!(replace(&mut *events.borrow_mut(), Vec::new()), $expected);
        };
    }

    new_view_tree! {
        let view0 = HView::new(ViewFlags::default());
        {
            let view1 = HView::new(ViewFlags::TAB_STOP);
        }
    }

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    flush_and_assert_events!([]);

    view1.focus();
    flush_and_assert_events!([
        (0, Event::FocusEnter),
        (1, Event::FocusEnter),
        (1, Event::FocusGot),
    ]);

    // Clear `ViewFlags::TAB_STOP`
    view1.set_flags(ViewFlags::default());

    // Should generate focus events
    flush_and_assert_events!([
        (1, Event::FocusLost),
        (1, Event::FocusLeave),
        (0, Event::FocusLeave),
    ]);

    // `is_focused` should return `false`
    assert_eq!([view0.is_focused(), view1.is_focused()], [false, false]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [false, false]
    );

    // `focused_view` should return `None`
    assert_eq!(wnd.focused_view(), None);
}

#[use_testing_wm]
#[test]
fn wnd_defocus(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    macro_rules! flush_and_assert_events {
        ($expected:expr) => {
            twm.step_unsend();
            assert_eq!(replace(&mut *events.borrow_mut(), Vec::new()), $expected);
        };
    }

    new_view_tree! {
        let view0 = HView::new(ViewFlags::default());
        {
            let view1 = HView::new(ViewFlags::TAB_STOP);
        }
    }

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    flush_and_assert_events!([]);

    // Set the focus. However, the events will not be raised because the window
    // is inactive.
    view1.focus();
    flush_and_assert_events!([]);

    assert_eq!([view0.is_focused(), view1.is_focused()], [false, false]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [false, false]
    );

    assert_eq!(wnd.focused_view(), Some(view1.clone()));

    // Focus the window
    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    flush_and_assert_events!([
        (0, Event::FocusEnter),
        (1, Event::FocusEnter),
        (1, Event::FocusGot),
    ]);

    assert_eq!([view0.is_focused(), view1.is_focused()], [false, true]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [true, true]
    );

    assert_eq!(wnd.focused_view(), Some(view1.clone()));

    // Unfocus the window
    twm.set_wnd_focused(&pal_hwnd, false);
    twm.step_unsend();

    flush_and_assert_events!([
        (1, Event::FocusLost),
        (1, Event::FocusLeave),
        (0, Event::FocusLeave),
    ]);

    assert_eq!([view0.is_focused(), view1.is_focused()], [false, false]);
    assert_eq!(
        [
            view0.improper_subview_is_focused(),
            view1.improper_subview_is_focused()
        ],
        [false, false]
    );

    assert_eq!(wnd.focused_view(), Some(view1.clone()));
}

#[use_testing_wm]
#[test]
fn access_focus_state_in_handler(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    new_view_tree! {
        let view0 = HView::new(ViewFlags::default());
        {
            let view1 = HView::new(ViewFlags::TAB_STOP);
        }
    }

    struct MyViewListener;
    impl ViewListener for MyViewListener {
        fn focus_enter(&self, _: pal::Wm, view: HViewRef<'_>) {
            // Reading the focus state must succeed (should not panic)
            let _ = view.is_focused();
        }
    }

    view0.set_listener(MyViewListener);
    view1.set_listener(MyViewListener);

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    view1.focus();
    twm.step_unsend();
}

#[use_testing_wm]
#[test]
fn focus_on_click_strong_focus(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    macro_rules! flush_and_assert_events {
        ($expected:expr) => {
            twm.step_unsend();
            assert_eq!(replace(&mut *events.borrow_mut(), Vec::new()), $expected);
        };
    }

    let view0 =
        HView::new(ViewFlags::TAB_STOP | ViewFlags::ACCEPT_MOUSE_DRAG | ViewFlags::STRONG_FOCUS);
    view0.set_listener(RecordingViewListener(0, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));
    view0.set_layout(EmptyLayout::new(
        SizeTraits::default().with_preferred([20.0; 2].into()),
    ));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    flush_and_assert_events!([]);

    // Click `view0`
    let _drag = twm.raise_mouse_drag(&pal_hwnd, view0.global_frame().mid(), 0);

    flush_and_assert_events!([(0, Event::FocusEnter), (0, Event::FocusGot),]);
}

#[use_testing_wm]
#[test]
fn focus_on_click_weak_focus(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    macro_rules! flush_and_assert_events {
        ($expected:expr) => {
            twm.step_unsend();
            assert_eq!(replace(&mut *events.borrow_mut(), Vec::new()), $expected);
        };
    }

    let view0 = HView::new(ViewFlags::TAB_STOP | ViewFlags::ACCEPT_MOUSE_DRAG);
    view0.set_listener(RecordingViewListener(0, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));
    view0.set_layout(EmptyLayout::new(
        SizeTraits::default().with_preferred([20.0; 2].into()),
    ));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    flush_and_assert_events!([]);

    // Click `view0`
    let _drag = twm.raise_mouse_drag(&pal_hwnd, view0.global_frame().mid(), 0);

    // It doesn't have `STRONG_FOCUS`, so it shouldn't receive a keyboard focus
    flush_and_assert_events!([]);
}
