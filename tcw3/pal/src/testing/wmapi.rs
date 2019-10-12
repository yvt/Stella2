use std::time::Instant;

/// Provides access to a virtual environment.
///
/// This is provided as a trait so that testing code can be compiled even
/// without a `testing` feature flag.
pub trait TestingWm: 'static {
    /// Get the global instance of [`tcw3::pal::Wm`]. This is identical to
    /// calling `Wm::global()`.
    ///
    /// [`tcw3::pal::Wm`]: crate::Wm
    fn wm(&self) -> crate::Wm;

    /// Process events until at least one event is processed.
    fn step(&self);

    /// Process events until at least one event is processed or
    /// until the specified instant.
    fn step_until(&self, till: Instant);
}
