use bitflags::bitflags;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{Matrix3, Point2, Rad, Vector2};
use rob::Rob;

use crate::pal::{LayerFlags, SysFontType, RGBAF32};

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
        /// The element is a splitter.
        const SPLITTER = 1 << 10;
        /// The element is a text entry widget.
        const ENTRY = 1 << 11;

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
    /// The smallest styling ID allocated for the system.
    pub const SYS_START_VALUE: u16 = 0xff80;
}

/// `ClassSet` of an element and its descendants.
pub type ElemClassPath = [ClassSet];

/// A role of a subview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, variant_count::VariantCount)]
pub enum Role {
    /// The default role.
    Generic = 0,
    HorizontalScrollbar,
    VerticalScrollbar,
    Bullet,
}

/// The number of roles defined by [`Role`].
///
/// The discriminant values of `Role` are guaranteed to range between
/// `0` and `ROLE_COUNT - 1`.
pub const ROLE_COUNT: usize = Role::VARIANT_COUNT as usize;

/// Represents a single styling property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prop {
    /// The number of layers.
    NumLayers,

    /// The [`HImg`] of the `n`-th layer.
    ///
    /// [`HImg`]: crate::images::HImg
    LayerImg(u32),

    /// The background color ([`RGBAF32`]) of the `n`-th layer.
    ///
    /// [`RGBAF32`]: crate::pal::RGBAF32
    LayerBgColor(u32),

    /// The [`Metrics`] of the `n`-th layer.
    LayerMetrics(u32),

    /// The opacity of the `n`-th layer.
    LayerOpacity(u32),

    /// The `content_center` of the `n`-th layer.
    LayerCenter(u32),

    /// The transformation of the `n`-th layer.
    LayerXform(u32),

    /// The flags of the `n`-th layer.
    LayerFlags(u32),

    /// The [`Metrics`] of a subview.
    SubviewMetrics(Role),

    /// Toggles the visibility of a subview.
    SubviewVisibility(Role),

    /// The [`Metrics`] of the layer used to clip subviews.
    ClipMetrics,

    /// The minimum size.
    MinSize,

    /// The default foreground color.
    FgColor,

    /// The default `SysFontType`.
    Font,
}

#[derive(Debug, Clone)]
pub enum PropValue {
    Bool(bool),
    Float(f32),
    Usize(usize),
    Himg(Option<crate::images::HImg>),
    Rgbaf32(RGBAF32),
    Metrics(Rob<'static, Metrics>),
    Vector2(Vector2<f32>),
    Point2(Point2<f32>),
    Box2(Box2<f32>),
    LayerXform(Rob<'static, LayerXform>),
    SysFontType(SysFontType),
    LayerFlags(LayerFlags),
}

impl PropValue {
    pub fn default_for_prop(prop: &Prop) -> Self {
        static DEFAULT_METRICS: Metrics = Metrics::default();
        match prop {
            Prop::NumLayers => PropValue::Usize(0),
            Prop::LayerImg(_) => PropValue::Himg(None),
            Prop::LayerBgColor(_) => PropValue::Rgbaf32(RGBAF32::new(0.0, 0.0, 0.0, 0.0)),
            Prop::LayerMetrics(_) => PropValue::Metrics(Rob::from_ref(&DEFAULT_METRICS)),
            Prop::LayerOpacity(_) => PropValue::Float(1.0),
            Prop::LayerCenter(_) => PropValue::Box2(box2! {
                min: [0.0, 0.0], max: [1.0, 1.0]
            }),
            Prop::LayerXform(_) => {
                static DEFAULT: LayerXform = LayerXform::default();
                PropValue::LayerXform(Rob::from_ref(&DEFAULT))
            }
            Prop::LayerFlags(_) => PropValue::LayerFlags(LayerFlags::default()),
            Prop::SubviewMetrics(_) => PropValue::Metrics(Rob::from_ref(&DEFAULT_METRICS)),
            Prop::SubviewVisibility(_) => PropValue::Bool(true),
            Prop::ClipMetrics => PropValue::Metrics(Rob::from_ref(&DEFAULT_METRICS)),
            Prop::MinSize => PropValue::Vector2(Vector2::new(0.0, 0.0)),
            Prop::FgColor => PropValue::Rgbaf32(RGBAF32::new(0.0, 0.0, 0.0, 1.0)),
            Prop::Font => PropValue::SysFontType(SysFontType::Normal),
        }
    }
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
