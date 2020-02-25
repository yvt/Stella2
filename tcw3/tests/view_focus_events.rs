use std::{cell::RefCell, mem::replace, rc::Rc};
use try_match::try_match;

use tcw3::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{layouts::TableLayout, AlignFlags},
    uicore::{HView, HViewRef, HWnd, ViewFlags, ViewListener},
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

    let view0 = HView::new(ViewFlags::default());
    let view1 = HView::new(ViewFlags::default() | ViewFlags::TAB_STOP);
    let view2 = HView::new(ViewFlags::default() | ViewFlags::TAB_STOP);
    let view3 = HView::new(ViewFlags::default() | ViewFlags::TAB_STOP);
    let view4 = HView::new(ViewFlags::default() | ViewFlags::TAB_STOP);

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));
    view2.set_listener(RecordingViewListener(2, events.clone()));
    view3.set_listener(RecordingViewListener(3, events.clone()));
    view4.set_listener(RecordingViewListener(4, events.clone()));

    view1.set_layout(new_layout(Some(view2.clone())));
    view3.set_layout(new_layout(Some(view4.clone())));

    view0.set_layout(new_layout(vec![view1.clone(), view3.clone()]));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    flush_and_assert_events!([]);

    // `view0` does not have `TAB_STOP`, so it won't accept a keyboard focus
    view0.focus();
    twm.raise_mouse_motion(&pal_hwnd, [0.0; 2].into());
    flush_and_assert_events!([]);

    // `view2` has a keyboard focus, which is a child of `view1`.
    // `view0` receives `mouse_enter` because of its subview receiving
    // `mouse_over`.
    view2.focus();
    flush_and_assert_events!([
        (0, Event::FocusEnter),
        (1, Event::FocusEnter),
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
fn has_focus(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    let view0 = HView::new(ViewFlags::default() | ViewFlags::TAB_STOP);
    let view1 = HView::new(ViewFlags::default() | ViewFlags::TAB_STOP);

    view0.set_listener(RecordingViewListener(0, events.clone()));
    view1.set_listener(RecordingViewListener(1, events.clone()));

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));
    view0.set_layout(new_layout(Some(view1.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    assert_eq!([view0.has_focus(), view1.has_focus()], [false, false]);
    assert_eq!(
        [
            view0.improper_subview_has_focus(),
            view1.improper_subview_has_focus()
        ],
        [false, false]
    );

    view0.focus();
    twm.step_unsend();

    assert_eq!([view0.has_focus(), view1.has_focus()], [true, false]);
    assert_eq!(
        [
            view0.improper_subview_has_focus(),
            view1.improper_subview_has_focus()
        ],
        [true, false]
    );

    view1.focus();
    twm.step_unsend();

    assert_eq!([view0.has_focus(), view1.has_focus()], [false, true]);
    assert_eq!(
        [
            view0.improper_subview_has_focus(),
            view1.improper_subview_has_focus()
        ],
        [true, true]
    );
}
