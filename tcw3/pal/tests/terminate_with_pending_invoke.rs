use log::{error, info};
use std::time::Duration;
use tcw3_pal::{prelude::*, Wm};

mod common;

fn main() {
    env_logger::init();
    common::set_timelimit_default();
    common::exit_if_native_backend_tests_are_disabled();

    let wm = Wm::global();

    wm.invoke_after(
        Duration::from_millis(100)..Duration::from_millis(1000),
        |wm| {
            info!("Calling `terminate`. The program should exit soon...");
            wm.terminate();
        },
    );
    wm.invoke_after(Duration::from_secs(5)..Duration::from_secs(5), |_| {
        error!("The program did not quit soon enough.");
        std::process::abort();
    });

    wm.enter_main_loop();
}
