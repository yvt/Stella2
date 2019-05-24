//! Provides standard UI components (views, layouts, ...).
pub mod layouts {
    mod abs;
    mod empty;
    mod fill;
    pub use self::{abs::*, empty::*, fill::*};
}

/// Reusable building blocks for creating UI components.
pub mod mixins {
    pub mod button;
    pub mod canvas;
    pub use self::{button::ButtonMixin, canvas::CanvasMixin};
}

pub mod views {
    mod button;
    mod label;
    pub use self::{button::Button, label::Label};
}
