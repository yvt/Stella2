use tcw3::{
    testing::{prelude::*, use_testing_wm},
    uicore::HWnd,
};
use try_match::try_match;

#[use_testing_wm]
#[test]
fn create_wnd(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);
    twm.step_unsend();
    assert_eq!(twm.hwnds().len(), 1);
    drop(wnd);
    assert_eq!(twm.hwnds().len(), 0);
}

#[use_testing_wm]
#[test]
fn close_wnd(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);
    twm.step_unsend();

    let pal_hwnd = try_match!([x] = twm.hwnds().as_slice() => x.clone())
        .expect("could not get a single window");
    twm.raise_close_requested(&pal_hwnd);

    // After receiving `close_requested`, `HWnd` should close itself
    // unless prevented by `uicore::WndListener::close_requested`.
    twm.step_unsend();
    assert_eq!(twm.hwnds().len(), 0);
}
