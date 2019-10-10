use bitflags::bitflags;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{Matrix3, Point2, Rad, Vector2};
use std::rc::Rc;

use crate::pal::{SysFontType, RGBAF32};

bitflags! {
    /// A set of styling classes.
    pub struct ClassSet: u32 {
        /// The mouse pointer inside the element.
        ///
        /// Be aware that this is a normal styling class like other ones. This
        /// does not get applied automatically like CSS's `:hover` pseudo
        /// selector.
        const HOVER = 1 << 0;
        /// The element is active, e.g., a button is being pressed down.
        const ACTIVE = 1 << 1;
        /// The element is a button's border.
        const BUTTON = 1 << 2;
        /// The element is a label.
        const LABEL = 1 << 3;
        /// The element is a scrollbar.
        const SCROLLBAR = 1 << 4;
        /// The element is vertical.
        const VERTICAL = 1 << 5;

        /// The bit mask for ID values. See [`ClassSet::id`] for more.
        const ID_MASK = 0xffff0000;
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
    /// How ID values are assigned is completely up to the application.
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
    pub const fn id(id: u16) -> Self {
        Self::from_bits_truncate((id as u32) << 16)
    }
}

/// `ClassSet` of an element and its ancestors.
#[derive(Debug, Clone)]
pub struct ElemClassPath {
    pub tail: Option<Rc<ElemClassPath>>,
    pub class_set: ClassSet,
}

impl ElemClassPath {
    pub fn new(class_set: ClassSet, tail: Option<Rc<ElemClassPath>>) -> Self {
        Self { tail, class_set }
    }
}

impl Default for ElemClassPath {
    fn default() -> Self {
        Self {
            tail: None,
            class_set: ClassSet::empty(),
        }
    }
}

/// A role of a subview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// The default role.
    Generic,
}

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

    /// The [`Metrics`] of a subview.
    SubviewMetrics(Role),

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
    Float(f32),
    Usize(usize),
    Himg(Option<crate::images::HImg>),
    Rgbaf32(RGBAF32),
    Metrics(Metrics),
    Vector2(Vector2<f32>),
    Point2(Point2<f32>),
    Box2(Box2<f32>),
    LayerXform(LayerXform),
    SysFontType(SysFontType),
}

impl PropValue {
    pub fn default_for_prop(prop: &Prop) -> Self {
        match prop {
            Prop::NumLayers => PropValue::Usize(0),
            Prop::LayerImg(_) => PropValue::Himg(None),
            Prop::LayerBgColor(_) => PropValue::Rgbaf32(RGBAF32::new(0.0, 0.0, 0.0, 0.0)),
            Prop::LayerMetrics(_) => PropValue::Metrics(Metrics::default()),
            Prop::LayerOpacity(_) => PropValue::Float(1.0),
            Prop::LayerCenter(_) => PropValue::Box2(box2! {
                min: [0.0, 0.0], max: [1.0, 1.0]
            }),
            Prop::LayerXform(_) => PropValue::LayerXform(LayerXform::default()),
            Prop::SubviewMetrics(_) => PropValue::Metrics(Metrics::default()),
            Prop::ClipMetrics => PropValue::Metrics(Metrics::default()),
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

impl Default for Metrics {
    fn default() -> Self {
        Self {
            margin: [0.0; 4],
            size: [std::f32::NAN; 2].into(),
        }
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

impl Default for LayerXform {
    fn default() -> Self {
        Self {
            anchor: [0.5; 2].into(),
            scale: [1.0; 2],
            rotate: Rad(0.0),
            translate: [0.0; 2].into(),
        }
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
