//! Provides a retained-mode rendering API that closely follows
//! the layering API provided by `tcw3::pal::iface::Wm`. However, this doesn't
//! have a method for applying deferred changes, and the client must explicitly
//! call a render method to get a rasterized image.
use iterpool::{Pool, PoolPtr};

/// The window handle type of [`Screen`].
#[derive(Debug, Clone)]
pub struct HWnd {
    ptr: PoolPtr,
}

/// The layer handle type of [`Screen`].
#[derive(Debug, Clone)]
pub struct HLayer {
    ptr: PoolPtr,
}

/// Manages layers and windows.
#[derive(Debug)]
pub struct Screen {
    layers: Pool<()>,
    windows: Pool<()>,
}

impl Screen {
    pub fn new() -> Self {
        unimplemented!()
    }
}
