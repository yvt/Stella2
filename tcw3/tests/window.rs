use enclose::enc;
use std::{cell::Cell, rc::Rc};
use try_match::try_match;

use tcw3::{
    pal,
    prelude::*,
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

#[use_testing_wm]
#[test]
fn invoke_on_update(twm: &dyn TestingWm) {
    let wm = twm.wm();
    let wnd = HWnd::new(wm);
    wnd.set_visibility(true);

    let count = Rc::new(Cell::new(0));
    fn incr_count(wm: pal::Wm, count: Rc<Cell<u32>>) {
        wm.invoke(enc!((count) move |_| {
            // Our framework is supposed to empty the queue of `invoke_on_update`
            // before calling `Wm::update_wnd`. It should be impossible to observe
            // `count` being in an intermediate state here.
            // (Ideally, we should check this property in `Wm::update_wnd`.)
            assert_eq!(count.get(), 3);
        }));

        if count.get() < 3 {
            count.set(count.get() + 1);
            wm.invoke_on_update(move |wm| incr_count(wm, count));
        }
    }
    wm.invoke_on_update(enc!((count) move |wm| incr_count(wm, count)));

    twm.step_unsend();

    assert_eq!(count.get(), 3);
}
