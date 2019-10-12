use std::time::{Duration, Instant};
use tcw3_pal::testing;

#[test]
fn create_testing_wm() {
    testing::run_test(|twm| {
        // This block might or might not run depending on a feature flag
        twm.step_until(Instant::now() + Duration::from_millis(100));
    });
}
