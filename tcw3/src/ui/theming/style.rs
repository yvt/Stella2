use bitflags::bitflags;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{Matrix3, Point2, Rad, Vector2};
use rob::Rob;

use crate::{
    pal::{LayerFlags, SysFontType, RGBAF32},
    ui::AlignFlags,
};

bitflags! {
    /// A set of styling classes.
    ///
    /// Note that they are all normal styling classes. For example, `HOVER`
    /// does not get applied automatically like CSS's `:hover` pseudo
    /// selector.
    pub struct ClassSet: u32 {
        /// The mouse pointer inside the element.
        const HOVER = 1;
        /// The element is active, e.g., a button is being pressed down.
        const ACTIVE = 1 << 1;
        /// The element is focused.
        const FOCUS = 1 << 2;
        /// The element is a button's border.
        const BUTTON = 1 << 3;
        /// The element is a label.
        const LABEL = 1 << 4;
        /// The element is a scrollbar.
        const SCROLLBAR = 1 << 5;
        /// The element is vertical.
        const VERTICAL = 1 << 6;
        /// The element is a scrollable container.
        const SCROLL_CONTAINER = 1 << 7;
        /// The scrollable container has a horizontal scrollbar.
        const HAS_HORIZONTAL_SCROLLBAR = 1 << 8;
        /// The scrollable container has a vertical scrollbar.
        const HAS_VERTICAL_SCROLLBAR = 1 << 9;
        /// The element is a text entry widget.
        const ENTRY = 1 << 11;
        /// The element is a checkbox widget.
        const CHECKBOX = 1 << 12;
        /// The element is checked.
        const CHECKED = 1 << 13;
        /// The element is a radio button widget.
        const RADIO_BUTTON = 1 << 14;

        const USER1 = 1 << 15;

        /// The bit mask for ID values. See [`ClassSet::id`] for more.
        const ID_MASK = 0xffff_0000;
    }
}

impl ClassSet {
    /// Construct an ID value.
    ///
    /// For design purposes, we want to identify every specific element on
    /// a window, but doing so by allocating a single bit for every element
    /// won't scale well. The solution is to reserve the upper bits for element
    /// ID.
    ///
    /// `ID_MASK` represents the set of bits allocated for ID values. This
    /// function creates a bit pattern representing an ID value using a subset
    /// of `ID_MASK`.
    ///
    /// How ID values are assigned is completely up to the application. However,
    /// the application should use values smaller than
    /// [`elem_id::SYS_START_VALUE`] to prevent collision.
    ///
    /// [`elem_id::SYS_START_VALUE`]: self::elem_id::SYS_START_VALUE
    ///
    /// # Examples
    ///
    /// ```
    /// use tcw3::ui::theming::ClassSet;
    ///
    /// const GO_BACK: ClassSet = ClassSet::id(0);
    /// const GO_FORWARD: ClassSet = ClassSet::id(1);
    ///
    /// let class1 = ClassSet::BUTTON | GO_BACK;
    /// let class2 = ClassSet::BUTTON | GO_FORWARD;
    ///
    /// assert_eq!(class1 & ClassSet::ID_MASK, GO_BACK);
    ///
    /// // Don't do this - the resulting bit pattern does not make sense:
    /// let bad = ClassSet::BUTTON | GO_BACK | GO_FORWARD;
    /// ```
    ///
    /// Not entire the representable range of `u16` can be used as an ID value:
    ///
    /// ```compile_fail
    /// # use tcw3::ui::theming::ClassSet;
    /// const INVALID: ClassSet = ClassSet::id(0xffff);
    /// ```
    pub const fn id(id: u16) -> Self {
        // Use multiplication to detect overflow at compile time
        Self::from_bits_truncate((id as u32 + 1) * (1u32 << 16))
    }
}

/// Styling IDs ([`ClassSet::id`]) reserved for the system.
pub mod elem_id {
    use super::*;
    /// The smallest styling ID allocated for the system.
    pub const SYS_START_VALUE: u16 = 0xff80;

    iota::iota! {
        pub const SPLITTER: ClassSet = ClassSet::id(iota + SYS_START_VALUE);
    }
}

/// `ClassSet` of an element and its descendants.
pub type ElemClassPath = [ClassSet];

/// Role of a subview.
///
/// The meaning of specific values is not defined by the framework. The
/// application developers may assign any values provided that they do not
/// conflict with each other in the same styling element.
///
/// Use [`roles::GENERIC`] when there are no subviews to be distinguished from
/// another.
///
/// [`roles::GENERIC`]: self::roles::GENERIC
pub type Role = u32;

/// Roles
pub mod roles {
    iota::iota! {
        pub const GENERIC: super::Role = iota;
                , HORZ_SCROLLBAR
                , VERT_SCROLLBAR
    }
}

#[macro_use]
mod prop_macros; // `def_prop!`

static DEFAULT_METRICS: Metrics = Metrics::default();

/// Zero-based layer index in range `0..num_layers` (where `num_layers` is the
/// computed value of the prop [`NumLayers`]).
///
/// [`NumLayers`]: self::Prop::NumLayers
pub type LayerId = u32;

/// Zero-based column index.
pub type Col = u32;

/// Zero-based row index.
pub type Row = u32;

def_prop! {
    /// Represents a single styling property.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Prop {
        /// The number of layers.
        #[snake_case(num_layers)]
        #[default(PropValue::Usize(0))]
        NumLayers,

        /// The [`HImg`] of the `n`-th layer.
        ///
        /// [`HImg`]: crate::images::HImg
        #[snake_case(layer_img)]
        #[default(PropValue::Himg(None))]
        LayerImg(LayerId),

        /// The background color ([`RGBAF32`]) of the `n`-th layer.
        ///
        /// [`RGBAF32`]: crate::pal::RGBAF32
        #[snake_case(layer_bg_color)]
        #[default(PropValue::Rgbaf32(RGBAF32::new(0.0, 0.0, 0.0, 0.0)))]
        LayerBgColor(LayerId),

        /// The [`Metrics`] of the `n`-th layer.
        #[snake_case(layer_metrics)]
        #[default(PropValue::Metrics(Rob::from_ref(&DEFAULT_METRICS)))]
        LayerMetrics(LayerId),

        /// The opacity of the `n`-th layer.
        #[snake_case(layer_opacity)]
        #[default(PropValue::Float(1.0))]
        LayerOpacity(LayerId),

        /// The `content_center` of the `n`-th layer.
        #[snake_case(layer_center)]
        #[default(PropValue::Box2(box2! {
            min: [0.0, 0.0], max: [1.0, 1.0]
        }))]
        LayerCenter(LayerId),

        /// The transformation of the `n`-th layer.
        #[snake_case(layer_xform)]
        #[default(PropValue::LayerXform({
            static DEFAULT: LayerXform = LayerXform::default();
            Rob::from_ref(&DEFAULT)
        }))]
        LayerXform(LayerId),

        /// The flags of the `n`-th layer.
        #[snake_case(layer_flags)]
        #[default(PropValue::LayerFlags(LayerFlags::default()))]
        LayerFlags(LayerId),

        /// The layout algorithm for subviews. Defaults to [`Layouter::Abs`].
        #[snake_case(subview_layouter)]
        #[default(PropValue::Layouter(Layouter::Abs))]
        SubviewLayouter,

        /// The padding for subviews.
        /// Only valid when [`Layouter::Table`] is the layouter.
        #[snake_case(subview_padding)]
        #[default(PropValue::F32x4([0.0; 4]))]
        SubviewPadding,

        /// The [`Metrics`] of a subview.
        /// Only valid when [`Layouter::Abs`] is the layouter.
        #[snake_case(subview_metrics)]
        #[default(PropValue::Metrics(Rob::from_ref(&DEFAULT_METRICS)))]
        SubviewMetrics(Role),

        /// The table cell to place a subview.
        /// Only valid when [`Layouter::Table`] is the layouter.
        #[snake_case(subview_table_cell)]
        #[default(PropValue::U32x2([0, 0]))]
        SubviewTableCell(Role),

        /// The alignment flags of a subview.
        /// Only valid when [`Layouter::Table`] is the layouter.
        #[snake_case(subview_table_align)]
        #[default(PropValue::AlignFlags(AlignFlags::CENTER))]
        SubviewTableAlign(Role),

        /// The inter-column spacing between two columns `i` and `i + 1`.
        /// Only valid when [`Layouter::Table`] is the layouter.
        #[snake_case(subview_table_col_spacing)]
        #[default(PropValue::Float(0.0))]
        SubviewTableColSpacing(Col),

        /// The inter-row spacing between two rows `i` and `i + 1`.
        /// Only valid when [`Layouter::Table`] is the layouter.
        #[snake_case(subview_table_row_spacing)]
        #[default(PropValue::Float(0.0))]
        SubviewTableRowSpacing(Row),

        /// Toggles the visibility of a subview.
        #[snake_case(subview_visibility)]
        #[default(PropValue::Bool(true))]
        SubviewVisibility(Role),

        /// The [`Metrics`] of the layer used to clip subviews.
        #[snake_case(clip_metrics)]
        #[default(PropValue::Metrics(Rob::from_ref(&DEFAULT_METRICS)))]
        ClipMetrics,

        /// The minimum size.
        #[snake_case(min_size)]
        #[default(PropValue::Vector2(Vector2::new(0.0, 0.0)))]
        MinSize,

        /// Expandability for each axis.
        #[snake_case(allow_grow)]
        #[default(PropValue::Bool2([true; 2]))]
        AllowGrow,

        /// The default foreground color.
        #[snake_case(fg_color)]
        #[default(PropValue::Rgbaf32(RGBAF32::new(0.0, 0.0, 0.0, 1.0)))]
        FgColor,

        /// The default background color.
        #[snake_case(bg_color)]
        #[default(PropValue::Rgbaf32(RGBAF32::new(1.0, 1.0, 1.0, 1.0)))]
        BgColor,

        /// The default `SysFontType`.
        #[snake_case(font)]
        #[default(PropValue::SysFontType(SysFontType::Normal))]
        Font,
    }
}

#[derive(Debug, Clone)]
pub enum PropValue {
    Bool(bool),
    Bool2([bool; 2]),
    Float(f32),
    Usize(usize),
    U32x2([u32; 2]),
    F32x4([f32; 4]),
    Himg(Option<crate::images::HImg>),
    Rgbaf32(RGBAF32),
    Metrics(Rob<'static, Metrics>),
    Vector2(Vector2<f32>),
    Point2(Point2<f32>),
    Box2(Box2<f32>),
    LayerXform(Rob<'static, LayerXform>),
    SysFontType(SysFontType),
    LayerFlags(LayerFlags),
    Layouter(Layouter),
    AlignFlags(AlignFlags),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layouter {
    /// Positions subviews using [`Metrics`].
    Abs,
    /// Positions subviews using an algorithm similar to [`TableLayout`].
    ///
    /// [`TableLayout`]: crate::ui::layouts::TableLayout
    Table,
}

/// Describes the placement of a rectangle (e.g., layer) inside a container.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    /// Distances from corresponding edges. Non-finite values (e.g., NaN) mean
    /// flexible space. Edges are specified in the clock-wise order, starting
    /// from top.
    pub margin: [f32; 4],
    /// The size of a layer. Non-finite values (e.g., NaN) mean the size is
    /// unspecified.
    pub size: Vector2<f32>,
}

impl Metrics {
    pub const fn default() -> Self {
        Self {
            margin: [0.0; 4],
            size: Vector2::new(std::f32::NAN, std::f32::NAN),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::default()
    }
}

impl Metrics {
    pub(crate) fn arrange(&self, container: Box2<f32>, default_size: Vector2<f32>) -> Box2<f32> {
        let mut kid_size = self.size;
        if !kid_size.x.is_finite() {
            kid_size.x = default_size.x;
        }
        if !kid_size.y.is_finite() {
            kid_size.y = default_size.y;
        }

        let mut frame = container;

        let margin = self.margin;
        if margin[3].is_finite() {
            frame.min.x += margin[3];
        }
        if margin[1].is_finite() {
            frame.max.x -= margin[1];
        }
        match (margin[3].is_finite(), margin[1].is_finite()) {
            (false, false) => {
                let mid = (frame.min.x + frame.max.x) * 0.5;
                frame.min.x = mid - kid_size.x * 0.5;
                frame.max.x = mid + kid_size.x * 0.5;
            }
            (true, false) => frame.max.x = frame.min.x + kid_size.x,
            (false, true) => frame.min.x = frame.max.x - kid_size.x,
            (true, true) => {}
        }

        if margin[0].is_finite() {
            frame.min.y += margin[0];
        }
        if margin[2].is_finite() {
            frame.max.y -= margin[2];
        }
        match (margin[0].is_finite(), margin[2].is_finite()) {
            (false, false) => {
                let mid = (frame.min.y + frame.max.y) * 0.5;
                frame.min.y = mid - kid_size.y * 0.5;
                frame.max.y = mid + kid_size.y * 0.5;
            }
            (true, false) => frame.max.y = frame.min.y + kid_size.y,
            (false, true) => frame.min.y = frame.max.y - kid_size.y,
            (true, true) => {}
        }

        frame
    }
}

/// Represents transformation of a layer.
#[derive(Debug, Clone, Copy)]
pub struct LayerXform {
    /// The anchor point, which is a fixed point of rotation and scaling.
    ///
    /// The point is specified relative to the layer's bounding rectangle.
    /// `[0, 0]` and `[1, 1]` represent the upper-left and lower-right corners,
    /// respectively.
    pub anchor: Point2<f32>,
    /// The scaling factor.
    pub scale: [f32; 2],
    /// The rotation angle.
    pub rotate: Rad<f32>,
    /// The translation vector, measured in points.
    pub translate: Vector2<f32>,
}

impl LayerXform {
    pub const fn default() -> Self {
        Self {
            anchor: Point2::new(0.5, 0.5),
            scale: [1.0; 2],
            rotate: Rad(0.0),
            translate: Vector2::new(0.0, 0.0),
        }
    }
}

impl Default for LayerXform {
    fn default() -> Self {
        Self::default()
    }
}

impl LayerXform {
    pub fn to_matrix3(&self, bounds: Box2<f32>) -> Matrix3<f32> {
        let anchor = Vector2::new(
            bounds.min.x + bounds.size().x * self.anchor.x,
            bounds.min.y + bounds.size().y * self.anchor.y,
        );
        Matrix3::from_translation(self.translate + anchor)
            * Matrix3::from_angle(self.rotate)
            * Matrix3::from_nonuniform_scale_2d(self.scale[0], self.scale[1])
            * Matrix3::from_translation(-anchor)
    }
}
