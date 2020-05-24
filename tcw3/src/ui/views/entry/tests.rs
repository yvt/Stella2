use crate::{
    pal,
    testing::{prelude::*, use_testing_wm},
    ui::{
        layouts::{EmptyLayout, TableLayout},
        theming::Manager,
        views::Spacer,
        AlignFlags,
    },
    uicore::{HView, HWnd, SizeTraits, ViewFlags},
};
use cggeom::prelude::*;
use enclose::enc;
use log::info;
use std::{cell::RefCell, rc::Rc};
use try_match::try_match;

use super::Entry;

fn simulate_click(twm: &dyn TestingWm, pal_hwnd: &pal::HWnd, p: cgmath::Point2<f32>) {
    info!("clicking at {:?}", p);
    let drag = twm.raise_mouse_drag(&pal_hwnd, p, 0);
    drag.mouse_down(p, 0);
    twm.step_unsend();
    drag.mouse_up(p, 0);
}

#[use_testing_wm(testing = "crate::testing")]
#[test]
fn text_input_ctx_activation(twm: &dyn TestingWm) {
    let wm = twm.wm();

    let style_manager = Manager::global(wm);

    let entry = Entry::new(wm, style_manager);
    let empty_view =
        HView::new(ViewFlags::TAB_STOP | ViewFlags::ACCEPT_MOUSE_DRAG | ViewFlags::STRONG_FOCUS);
    empty_view.set_layout(EmptyLayout::new(
        SizeTraits::default().with_min([0.0, 20.0].into()),
    ));

    let wnd = HWnd::new(wm);
    wnd.content_view().set_layout(TableLayout::stack_vert(vec![
        (entry.view(), AlignFlags::JUSTIFY),
        (empty_view.clone(), AlignFlags::JUSTIFY),
        (
            Spacer::new().with_min([100.0, 0.0]).into_view(),
            AlignFlags::JUSTIFY,
        ),
    ]));
    wnd.set_visibility(true);

    twm.step_unsend();

    // Focus the window
    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    // No widget is focused at the start
    assert!(twm.expect_unique_active_text_input_ctx().is_none());

    // Focus the text field by clicking it
    let bounds = entry.view_ref().global_frame();
    simulate_click(twm, &pal_hwnd, bounds.min.average2(&bounds.min));

    assert!(twm.expect_unique_active_text_input_ctx().is_some());

    // Focus the other widget by clicking it
    let bounds = empty_view.global_frame();
    simulate_click(twm, &pal_hwnd, bounds.min.average2(&bounds.min));

    assert!(twm.expect_unique_active_text_input_ctx().is_none());
}

#[allow(dead_code)]
struct TestWithOneEntry {
    wm: pal::Wm,
    hwnd: HWnd,
    pal_hwnd: pal::HWnd,
    entry: Rc<Entry>,
    changed_events: Rc<RefCell<Vec<String>>>,
}
fn init_test_with_one_entry(twm: &dyn TestingWm) -> TestWithOneEntry {
    let wm = twm.wm();

    let style_manager = Manager::global(wm);

    let entry = Rc::new(Entry::new(wm, style_manager));

    let wnd = HWnd::new(wm);
    wnd.content_view().set_layout(TableLayout::stack_vert(vec![
        (entry.view(), AlignFlags::JUSTIFY),
        (
            Spacer::new().with_min([100.0, 0.0]).into_view(),
            AlignFlags::JUSTIFY,
        ),
    ]));
    wnd.set_visibility(true);

    twm.step_unsend();

    // Focus the window
    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    // Register a `changed` event handler
    let changed_events = Rc::new(RefCell::new(Vec::new()));
    let entry_weak = Rc::downgrade(&entry);
    entry.subscribe_changed(Box::new(enc!((changed_events) move |_| {
        if let Some(entry) = entry_weak.upgrade() {
            changed_events.borrow_mut().push(entry.text());
        }
    })));

    TestWithOneEntry {
        wm,
        hwnd: wnd,
        pal_hwnd,
        entry,
        changed_events,
    }
}

#[use_testing_wm(testing = "crate::testing")]
#[test]
fn set_text(twm: &dyn TestingWm) {
    let TestWithOneEntry {
        entry,
        hwnd: _hwnd,
        pal_hwnd,
        changed_events,
        ..
    } = init_test_with_one_entry(twm);

    // Focus the text field by clicking it
    let bounds = entry.view_ref().global_frame();
    simulate_click(twm, &pal_hwnd, bounds.min.average2(&bounds.min));

    // Type something
    {
        let mut edit = twm.raise_edit(&twm.expect_unique_active_text_input_ctx().unwrap(), true);
        edit.replace(0..0, "hello");
    }
    twm.step_unsend();

    assert!(entry.core().inner.state.borrow().history.can_undo());
    assert_eq!(entry.text(), "hello");
    assert_eq!(changed_events.borrow()[..], ["hello"][..]);

    // Assign a new text
    entry.set_text("world");
    twm.step_unsend();

    // The new text should be there
    assert_eq!(entry.text(), "world");

    // .. and that history should be forgotten
    assert!(!entry.core().inner.state.borrow().history.can_undo());

    // .. and a `changed` event should be generated
    assert_eq!(changed_events.borrow()[..], ["hello", "world"][..]);
}
