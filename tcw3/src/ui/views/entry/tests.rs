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
use log::info;
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

    let entry = Entry::new(style_manager);
    let empty_view = HView::new(ViewFlags::TAB_STOP | ViewFlags::ACCEPT_MOUSE_DRAG);
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
