use try_match::try_match;

use tcw3::{
    testing::{prelude::*, use_testing_wm},
    ui::{layouts::TableLayout, AlignFlags},
    uicore::{HView, HWnd, ViewFlags},
};

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
fn tabbing(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);

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

    wnd.content_view()
        .set_layout(new_layout(Some(view0.clone())));

    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");

    twm.set_wnd_focused(&pal_hwnd, true);
    twm.step_unsend();

    let tab_order = [&view1, &view2, &view3, &view4];

    log::debug!("Expected tab order: {:#?}", tab_order);

    // Cycle through the tab order
    let actual_tab_order: Vec<_> = (0..tab_order.len() * 3)
        .map(|_| {
            twm.simulate_key(&pal_hwnd, "windows", "Tab");
            twm.step_unsend();
            wnd.focused_view().unwrap()
        })
        .collect();

    // Cycle through the tab order in a reverse order
    let actual_tab_order_rev: Vec<_> = (0..tab_order.len() * 3 - 1)
        .map(|_| {
            twm.simulate_key(&pal_hwnd, "windows", "Shift+Tab");
            twm.step_unsend();
            wnd.focused_view().unwrap()
        })
        .collect();

    let expected_tab_order: Vec<_> = tab_order
        .iter()
        .cycle()
        .take(actual_tab_order.len())
        .cloned()
        .cloned()
        .collect();

    assert_eq!(actual_tab_order, expected_tab_order);

    assert_eq!(
        actual_tab_order_rev,
        expected_tab_order
            .into_iter()
            .rev()
            .skip(1)
            .collect::<Vec<_>>()
    );
}
