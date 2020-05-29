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
    pub mod scrollwheel;
    pub use self::{button::ButtonMixin, canvas::CanvasMixin, scrollwheel::ScrollWheelMixin};
}

pub mod views {
    mod button;
    mod checkbox;
    mod entry;
    mod label;
    pub mod scrollbar;
    pub mod slider;
    mod spacer;
    pub mod split;
    pub mod table;
    pub use self::{
        button::Button,
        checkbox::{Checkbox, RadioButton},
        entry::{Entry, EntryCore},
        label::Label,
        scrollbar::ScrollbarRaw,
        slider::{Slider, SliderRaw},
        spacer::{new_spacer, Spacer},
        split::Split,
        table::{ScrollableTable, Table},
    };
    tcw3_meta::designer_impl! { crate::ui::views::SpacerWidget }
    tcw3_meta::designer_impl! { crate::ui::views::FixedSpacer }
}

/// Theming support
pub mod theming {
    mod manager;
    mod style;
    mod stylesheet;
    mod view;
    mod widget;

    pub use self::{
        manager::{Elem, ElemChangeCb, HElem, Manager, PropKindFlags},
        style::{
            elem_id, mk_prop_by_snake_name, mk_prop_value_by_prop_snake_name,
            mk_wrap_dynvalue_by_prop_snake_name, mk_wrap_value_by_prop_snake_name, roles, ClassSet,
            Col, ElemClassPath, GetPropValue, LayerId, LayerXform, Layouter, Metrics, Prop,
            PropValue, Role, Row,
        },
        stylesheet::*,
        view::{ModifyArrangementArgs, StyledBox, StyledBoxOverride},
        widget::Widget,
    };
}

mod types;
pub use self::types::AlignFlags;

mod scrolling {
    pub mod lineset;
    pub mod piecewise;
    pub mod tableremap;
}

/// Text editing support
pub mod editing {
    pub mod history;
}

/// Re-exports some traits from the `ui` module.
pub mod prelude {
    pub use super::{
        theming::GetPropValue,
        views::table::{TableModelEdit, TableModelEditExt},
    };
}
