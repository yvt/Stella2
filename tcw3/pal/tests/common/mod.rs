#![allow(dead_code)]
use std::{env::var_os, time::Duration};

pub fn set_timelimit_default() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(30));
        eprintln!("!!! Time limit exceeed.");
        std::process::abort();
    });
}

pub fn exit_if_native_backend_tests_are_disabled() {
    if let Some(value) = var_os("ST_SKIP_NATIVE_BACKEND_TESTS") {
        if !value.is_empty() && value != "0" {
            println!("Skipping because ST_SKIP_NATIVE_BACKEND_TESTS is set");
            std::process::exit(0);
        }
    }
}

pub fn try_init_logger_for_default_harness() {
    let _ = env_logger::builder().is_test(true).try_init();
}
