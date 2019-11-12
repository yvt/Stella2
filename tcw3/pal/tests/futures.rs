use futures::{channel::oneshot::channel, task::LocalSpawnExt};
use std::{env::var_os, time::Duration};
use tcw3_pal::{prelude::Wm as _, prelude::*, Wm};

fn main() {
    env_logger::init();

    if let Some(value) = var_os("ST_SKIP_NATIVE_BACKEND_TESTS") {
        if !value.is_empty() && value != "0" {
            println!("Skipping the test because ST_SKIP_NATIVE_BACKEND_TESTS is set");
            return;
        }
    }

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
