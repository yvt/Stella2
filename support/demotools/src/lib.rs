//! Utilities for example programs
use std::time::Instant;

/// Measures the production rate of some quantity.
#[derive(Debug)]
pub struct RateCounter {
    last_measure: Instant,
    count: f64,
    last_rate: f64,
}

impl RateCounter {
    pub fn new() -> Self {
        Self {
            last_measure: Instant::now(),
            count: 0.0,
            last_rate: 0.0,
        }
    }

    /// Log a quantity.
    ///
    /// Returns `true` if the value of `rate()` is updated.
    pub fn log(&mut self, value: f64) -> bool {
        self.count += value;

        let dt = self.last_measure.elapsed().as_secs_f64();
        if dt >= 0.2 {
            self.last_rate = self.count / dt;
            self.count = 0.0;
            self.last_measure = Instant::now();
            true
        } else {
            false
        }
    }

    /// Get the measured rate (`value` per second).
    pub fn rate(&self) -> f64 {
        self.last_rate
    }
}
