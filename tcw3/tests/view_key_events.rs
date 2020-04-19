use std::{cell::RefCell, mem::replace, rc::Rc};
use try_match::try_match;

use tcw3::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{layouts::TableLayout, AlignFlags},
    uicore::{
        ActionId, ActionStatus, HView, HViewRef, HWnd, HWndRef, KeyEvent, ViewFlags, ViewListener,
        WndListener,
    },
};

#[derive(Debug, PartialEq)]
enum Event {
    Action,
    KeyDown,
    KeyUp,
}

struct RecVL(usize, bool, Rc<RefCell<Vec<(usize, Event)>>>);

impl ViewListener for RecVL {
    fn validate_action(&self, _: pal::Wm, _: HViewRef<'_>, action: ActionId) -> ActionStatus {
        if action == 42 {
            ActionStatus::VALID | ActionStatus::ENABLED
        } else {
            unreachable!();
        }
    }

    fn perform_action(&self, _: pal::Wm, _: HViewRef<'_>, action: ActionId) {
        assert_eq!(action, 42);
        self.2.borrow_mut().push((self.0, Event::Action));
    }

    fn key_down(&self, _: pal::Wm, _: HViewRef<'_>, _: &KeyEvent<'_>) -> bool {
        if self.1 {
            self.2.borrow_mut().push((self.0, Event::KeyDown));
            true
        } else {
            false
        }
    }

    fn key_up(&self, _: pal::Wm, _: HViewRef<'_>, _: &KeyEvent<'_>) -> bool {
        if self.1 {
            self.2.borrow_mut().push((self.0, Event::KeyUp));
            true
        } else {
            false
        }
    }
}

impl WndListener for RecVL {
    fn interpret_event(
        &self,
        _: pal::Wm,
        _: HWndRef<'_>,
        ctx: &mut tcw3::uicore::InterpretEventCtx<'_>,
    ) {
        ctx.use_accel(&pal::accel_table![(42, windows("Ctrl+Q")),]);
    }

    fn key_down(&self, _: pal::Wm, _: HWndRef<'_>, _: &KeyEvent<'_>) -> bool {
        if self.1 {
            self.2.borrow_mut().push((self.0, Event::KeyDown));
            true
        } else {
            false
        }
    }

    fn key_up(&self, _: pal::Wm, _: HWndRef<'_>, _: &KeyEvent<'_>) -> bool {
        if self.1 {
            self.2.borrow_mut().push((self.0, Event::KeyUp));
            true
        } else {
            false
        }
    }
}

fn new_layout(views: impl IntoIterator<Item = HView>) -> TableLayout {
    TableLayout::stack_horz(views.into_iter().map(|v| (v, AlignFlags::JUSTIFY)))
}

fn init_test(
    twm: &dyn TestingWm,
    wnd_opts: bool,
    view_opts: Vec<bool>,
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
    let (_wnd, pal_hwnd, events) = init_test(twm, false, vec![true, true, true, false]);

    // The third view should receive the key stroke
    twm.simulate_key(&pal_hwnd, "windows", "Ctrl+S");
    twm.step_unsend();
    assert_eq!(
        replace(&mut *events.borrow_mut(), Vec::new()),
        [(3, Event::KeyDown), (3, Event::KeyUp)]
    );
}

#[use_testing_wm]
#[test]
fn wnd_enabled(twm: &dyn TestingWm) {
    let (_wnd, pal_hwnd, events) = init_test(twm, true, vec![false]);

    // The window should receive the action
    twm.simulate_key(&pal_hwnd, "windows", "Ctrl+S");
    twm.step_unsend();
    assert_eq!(
        replace(&mut *events.borrow_mut(), Vec::new()),
        [(0, Event::KeyDown), (0, Event::KeyUp)]
    );
}

#[use_testing_wm]
#[test]
fn actions_should_take_prescedence(twm: &dyn TestingWm) {
    let (_wnd, pal_hwnd, events) = init_test(twm, true, vec![true]);

    // `Ctrl+Q` is translated to the action 42, so `key_down` should never be
    // called in this case.
    twm.simulate_key(&pal_hwnd, "windows", "Ctrl+Q");
    twm.step_unsend();

    let events = replace(&mut *events.borrow_mut(), Vec::new());
    assert!(events.contains(&(1, Event::Action)));
    assert!(!events.contains(&(0, Event::KeyDown)) && !events.contains(&(1, Event::KeyDown)));
}
