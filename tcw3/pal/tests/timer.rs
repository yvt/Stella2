use futures::task::LocalSpawnExt;
use log::info;
use std::time::{Duration, Instant};
use tcw3_pal::{prelude::*, Wm};

mod common;

fn main() {
    env_logger::init();
    common::set_timelimit_default();
    common::exit_if_native_backend_tests_are_disabled();

    let wm = Wm::global();

    wm.spawner()
        .spawn_local(async move {
            let d_200_ms = Duration::from_millis(200);
            let d_600_ms = Duration::from_millis(600);
            let d_1200_ms = Duration::from_millis(1200);

            // Successful sleep operation
            let sleep1 = wm.sleep(d_600_ms..d_1200_ms);
            let sleep1_b = sleep1.clone();
            let start = Instant::now();
            assert!(sleep1.poll_without_context().is_pending());
            std::thread::sleep(d_200_ms);
            assert!(sleep1.poll_without_context().is_pending());
            sleep1.await.unwrap();

            info!(
                "sleep1 resolved after {:?} (expected to be in range {:?})",
                start.elapsed(),
                d_600_ms..d_1200_ms
            );

            // A completed sleep operation can't be cancelled anymore
            assert!(!sleep1_b.cancel());

            // Cancelled sleep operation
            let sleep2 = wm.sleep(d_600_ms..d_1200_ms);
            let start = Instant::now();
            assert!(sleep2.poll_without_context().is_pending());
            std::thread::sleep(d_200_ms);
            assert!(sleep2.poll_without_context().is_pending());
            assert!(sleep2.cancel());
            assert!(sleep2.poll_without_context().is_ready());
            assert!(!sleep2.cancel());
            sleep2.await.err().unwrap();

            info!(
                "sleep2 cancelled after {:?} (expected to be around {:?})",
                start.elapsed(),
                d_200_ms
            );

            println!("Test passed");
            wm.terminate();
        })
        .unwrap();

    wm.enter_main_loop();
}
