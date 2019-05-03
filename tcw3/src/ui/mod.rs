//! Provides standard UI components (views, layouts, ...).
pub mod layouts {
    mod abs;
    mod empty;
    mod fill;
    pub use self::{abs::*, empty::*, fill::*};
}
