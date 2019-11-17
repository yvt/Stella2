use futures::{channel::oneshot::channel, task::LocalSpawnExt};
use std::time::Duration;
use tcw3_pal::{prelude::Wm as _, prelude::*, Wm};

mod common;

fn main() {
    env_logger::init();
    common::set_timelimit_default();
    common::exit_if_native_backend_tests_are_disabled();

    let wm = Wm::global();

    let (send, recv) = channel();

    // Send a payload sometime later
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        send.send(()).unwrap();
    });

    // This task run on the main thread, and exits the program gracefully
    // upon receiving a payload via the channel
    wm.spawner()
        .spawn_local(async move {
            let () = recv.await.unwrap();
            println!("Received a payload (test passed)");

            wm.terminate();
        })
        .unwrap();

    wm.enter_main_loop();
}
