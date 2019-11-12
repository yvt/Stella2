use std::time::Duration;

pub fn set_timelimit_default() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(30));
        eprintln!("!!! Time limit exceeed.");
        std::process::abort();
    });
}
