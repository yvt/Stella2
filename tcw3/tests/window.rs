use tcw3::{
    testing::{prelude::*, use_testing_wm},
    uicore::HWnd,
};

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
