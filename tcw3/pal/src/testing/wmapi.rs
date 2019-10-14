use std::time::Instant;

use crate::{iface, HWnd};

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

    /// Get a list of currently open windows.
    fn hwnds(&self) -> Vec<HWnd>;

    /// Get the attributes of a window.
    fn wnd_attrs(&self, hwnd: &HWnd) -> Option<WndAttrs>;
}

/// A snapshot of window attributes.
#[derive(Debug, Clone)]
pub struct WndAttrs {
    pub size: [u32; 2],
    pub min_size: [u32; 2],
    pub max_size: [u32; 2],
    pub flags: iface::WndFlags,
    pub caption: String,
    pub visible: bool,
}
