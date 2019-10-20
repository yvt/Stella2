//! A helper library for `cgmath`.
//!
//! Provides additional types useful in computer graphics.
pub extern crate cgmath;

mod average;
mod boxes;
mod elementwise;
mod twodim;

pub use self::average::*;
pub use self::boxes::*;
pub use self::elementwise::*;
pub use self::twodim::*;

/// The prelude.
pub mod prelude {
    #[doc(no_inline)]
    pub use crate::{
        Average2, AxisAlignedBox, ElementWiseOp, ElementWisePartialOrd, Matrix3TwoDimExt,
    };
}
