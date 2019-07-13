//! Provides standard UI components (views, layouts, ...).
pub mod layouts {
    mod abs;
    mod empty;
    mod fill;
    mod table;
    pub use self::{abs::*, empty::*, fill::*, table::*};
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
    mod spacer;
    pub mod split;
    pub use self::{
        button::Button,
        label::Label,
        spacer::{new_spacer, Spacer},
        split::Split,
    };
}

/// Theming support
pub mod theming {
    mod manager;
    mod style;
    mod stylesheet;
    mod view;
    pub use self::{
        manager::{Elem, ElemChangeCb, Manager, PropKindFlags},
        style::{ClassSet, ElemClassPath, Metrics, Prop, PropValue, Role},
        stylesheet::*,
        view::StyledBox,
    };
}

mod types;
pub use self::types::{AlignFlags, Suspend, SuspendFlag, SuspendGuard};

mod scrolling {
    mod lineset;
    mod piecewise;
}
