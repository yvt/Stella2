//! A helper library for `cgmath`.
//!
//! Provides additional types useful in computer graphics.
extern crate cgmath;

mod boxes;
mod elementwise;

pub use self::boxes::*;
pub use self::elementwise::*;

/// The prelude.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{AxisAlignedBox, ElementWiseOp, ElementWisePartialOrd};
}
