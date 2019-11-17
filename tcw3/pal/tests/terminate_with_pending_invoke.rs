use log::{error, info};
use std::{env::var_os, time::Duration};
use tcw3_pal::{prelude::Wm as _, Wm};

mod common;

fn main() {
    env_logger::init();
    common::set_timelimit_default();

    if let Some(value) = var_os("ST_SKIP_NATIVE_BACKEND_TESTS") {
        if !value.is_empty() && value != "0" {
            println!("Skipping because ST_SKIP_NATIVE_BACKEND_TESTS is set");
            return;
        }
    }

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
