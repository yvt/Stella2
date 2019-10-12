use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    thread::spawn,
    time::{Duration, Instant},
};
use tcw3_pal::{iface::Wm as _, testing, MtLock, Wm};

#[test]
fn create_testing_wm() {
    testing::run_test(|twm| {
        // This block might or might not run depending on a feature flag
        twm.step_until(Instant::now() + Duration::from_millis(100));
    });
}

#[test]
fn invoke() {
    testing::run_test(|twm| {
        let flag = Rc::new(Cell::new(false));
        {
            let flag = Rc::clone(&flag);
            twm.wm().invoke(move |_| flag.set(true));
        }

        // Wait until the closure is called and the flag is set
        while !flag.get() {
            twm.step();
        }
    });
}

#[test]
fn invoke_on_main_thread() {
    testing::run_test(|twm| {
        let flag = Arc::new(MtLock::<_, Wm>::new(Cell::new(false)));

        {
            let flag = Arc::clone(&flag);
            spawn(move || {
                Wm::invoke_on_main_thread(move |wm| flag.get_with_wm(wm).set(true));
            });
        }

        // Wait until the closure is called and the flag is set
        while !flag.get_with_wm(twm.wm()).get() {
            twm.step();
        }
    });
}

#[test]
#[should_panic]
fn panicking() {
    testing::run_test(|twm| {
        panic!("this panic should be contained to this test case");
    });
}
