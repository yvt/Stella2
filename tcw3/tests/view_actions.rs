use std::{cell::RefCell, mem::replace, rc::Rc};
use try_match::try_match;

use tcw3::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{layouts::TableLayout, AlignFlags},
    uicore::{
        ActionId, ActionStatus, HView, HViewRef, HWnd, HWndRef, ViewFlags, ViewListener,
        WndListener,
    },
};

#[derive(Debug, PartialEq)]
enum Event {
    Action,
}

struct RecVL(usize, ActionStatus, Rc<RefCell<Vec<(usize, Event)>>>);

impl ViewListener for RecVL {
    fn validate_action(&self, _: pal::Wm, _: HViewRef<'_>, action: ActionId) -> ActionStatus {
        if action == 42 {
            self.1
        } else {
            unreachable!();
        }
    }

    fn perform_action(&self, _: pal::Wm, _: HViewRef<'_>, action: ActionId) {
        assert_eq!(action, 42);
        self.2.borrow_mut().push((self.0, Event::Action));
    }
}

impl WndListener for RecVL {
    fn validate_action(&self, _: pal::Wm, _: HWndRef<'_>, action: ActionId) -> ActionStatus {
        if action == 42 {
            self.1
        } else {
            unreachable!();
        }
    }

    fn perform_action(&self, _: pal::Wm, _: HWndRef<'_>, action: ActionId) {
        assert_eq!(action, 42);
        self.2.borrow_mut().push((self.0, Event::Action));
    }
}

fn new_layout(views: impl IntoIterator<Item = HView>) -> TableLayout {
    TableLayout::stack_horz(views.into_iter().map(|v| (v, AlignFlags::JUSTIFY)))
}

fn init_test(
    twm: &dyn TestingWm,
    wnd_opts: ActionStatus,
    view_opts: Vec<ActionStatus>,
) -> (HWnd, pal::HWnd, Rc<RefCell<Vec<(usize, Event)>>>) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

    let events = Rc::new(RefCell::new(Vec::new()));

    let views: Vec<_> = view_opts
        .iter()
        .enumerate()
        .map(|(i, &opt)| {
            let view = HView::new(ViewFlags::TAB_STOP);
            view.set_listener(RecVL(i + 1, opt, events.clone()));
            view
        })
        .collect();

    wnd.content_view()
        .set_layout(new_layout(Some(views[0].clone())));
    for views in views.windows(2) {
        views[0].set_layout(new_layout(Some(views[1].clone())));
    }

    wnd.set_listener(RecVL(0, wnd_opts, events.clone()));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    views.last().unwrap().focus();
    (wnd, pal_hwnd, events)
}

#[use_testing_wm]
#[test]
fn enabled(twm: &dyn TestingWm) {
    let (_wnd, pal_hwnd, events) = init_test(
        twm,
        ActionStatus::empty(),
        vec![
            ActionStatus::VALID | ActionStatus::ENABLED,
            ActionStatus::VALID | ActionStatus::ENABLED,
            ActionStatus::VALID | ActionStatus::ENABLED, // This will handle the action
            ActionStatus::empty(),
        ],
    );

    assert_eq!(
        twm.raise_validate_action(&pal_hwnd, 42),
        ActionStatus::VALID | ActionStatus::ENABLED
    );

    // The third view should receive the action
    twm.raise_perform_action(&pal_hwnd, 42);
    twm.step_unsend();
    assert_eq!(
        replace(&mut *events.borrow_mut(), Vec::new()),
        [(3, Event::Action)]
    );
}

#[use_testing_wm]
#[test]
fn wnd_enabled(twm: &dyn TestingWm) {
    let (_wnd, pal_hwnd, events) = init_test(
        twm,
        // This (window) will handle the action
        ActionStatus::VALID | ActionStatus::ENABLED,
        vec![ActionStatus::empty()],
    );

    assert_eq!(
        twm.raise_validate_action(&pal_hwnd, 42),
        ActionStatus::VALID | ActionStatus::ENABLED
    );

    // The window should receive the action
    twm.raise_perform_action(&pal_hwnd, 42);
    twm.step_unsend();
    assert_eq!(
        replace(&mut *events.borrow_mut(), Vec::new()),
        [(0, Event::Action)]
    );
}

#[use_testing_wm]
#[test]
fn disabled(twm: &dyn TestingWm) {
    let (_wnd, pal_hwnd, _) = init_test(
        twm,
        ActionStatus::empty(),
        vec![
            ActionStatus::VALID | ActionStatus::ENABLED,
            ActionStatus::VALID | ActionStatus::ENABLED,
            ActionStatus::VALID, // This will handle the action
            ActionStatus::empty(),
        ],
    );

    assert_eq!(
        twm.raise_validate_action(&pal_hwnd, 42),
        ActionStatus::VALID
    );
}
