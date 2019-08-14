use cgmath::prelude::*;
use cgmath::{
    num_traits::NumCast, AbsDiffEq, BaseFloat, BaseNum, Point2, Point3, UlpsEq, Vector2, Vector3,
};
use std::ops::Add;

use super::{BoolArray, ElementWiseOp, ElementWisePartialOrd};

pub trait AxisAlignedBox<T>: Sized {
    type Point: EuclideanSpace
        + ElementWiseOp
        + ElementWisePartialOrd
        + Add<Self::Vector, Output = Self::Point>;
    type Vector: Clone;

    fn new(min: Self::Point, max: Self::Point) -> Self;
    fn with_size(min: Self::Point, size: Self::Vector) -> Self {
        Self::new(min, min + size)
    }

    fn min(&self) -> Self::Point;
    fn max(&self) -> Self::Point;

    fn zero() -> Self;

    /// Return `true` if a point is inside a box.
    ///
    /// # Examples
    ///
    ///     use cggeom::{prelude::*, Box2};
    ///     use cgmath::Point2;
    ///
    ///     let b = Box2::new(
    ///         Point2::new(0.0, 0.0),
    ///         Point2::new(1.0, 1.0),
    ///     );
    ///     assert!(b.contains_point(&Point2::new(0.0, 0.0)));
    ///     assert!(b.contains_point(&Point2::new(0.5, 0.5)));
    ///
    ///     assert!(!b.contains_point(&Point2::new(0.5, -1.0)));
    ///     assert!(!b.contains_point(&Point2::new(0.5, 1.0)));
    ///     assert!(!b.contains_point(&Point2::new(-1.0, 0.5)));
    ///     assert!(!b.contains_point(&Point2::new(1.0, 0.5)));
    ///
    #[inline]
    fn contains_point(&self, point: &Self::Point) -> bool
    where
        T: PartialOrd,
    {
        point.element_wise_ge(&self.min()).all() && point.element_wise_lt(&self.max()).all()
    }

    fn is_valid(&self) -> bool;
    fn is_empty(&self) -> bool;

    #[inline]
    fn size(&self) -> <Self::Point as EuclideanSpace>::Diff
    where
        T: BaseNum,
    {
        self.max() - self.min()
    }

    #[inline]
    fn union(&self, other: &Self) -> Self
    where
        T: BaseNum,
    {
        Self::new(
            self.min().element_wise_min(&other.min()),
            self.max().element_wise_max(&other.max()),
        )
    }

    #[inline]
    fn union_assign(&mut self, other: &Self)
    where
        T: BaseNum,
    {
        *self = self.union(other);
    }

    #[inline]
    fn intersection(&self, other: &Self) -> Option<Self>
    where
        T: BaseNum,
    {
        let s = Self::new(
            self.min().element_wise_max(&other.min()),
            self.max().element_wise_min(&other.max()),
        );
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    #[inline]
    fn translate(&self, displacement: Self::Vector) -> Self {
        Self::new(
            self.min() + displacement.clone(),
            self.max() + displacement.clone(),
        )
    }
}

/// Represents an axis-aligned 2D box.
#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Box2<T> {
    /// The minimum coordinate (inclusive).
    pub min: Point2<T>,

    /// The maximum coordinate (exclusive).
    pub max: Point2<T>,
}

/// Represents an axis-aligned 3D box.
#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Box3<T> {
    /// The minimum coordinate (inclusive).
    pub min: Point3<T>,

    /// The maximum coordinate (exclusive).
    pub max: Point3<T>,
}

impl<T: BaseNum> AxisAlignedBox<T> for Box2<T> {
    type Point = Point2<T>;
    type Vector = Vector2<T>;

    #[inline]
    fn new(min: Self::Point, max: Self::Point) -> Self {
        Self { min: min, max: max }
    }

    #[inline]
    fn is_valid(&self) -> bool {
        let size = self.size();
        size.x >= T::zero() && size.y >= T::zero()
    }
    #[inline]
    fn is_empty(&self) -> bool {
        let size = self.size();
        size.x <= T::zero() && size.y <= T::zero()
    }

    #[inline]
    fn zero() -> Self {
        Self::new(
            Point2::new(T::zero(), T::zero()),
            Point2::new(T::zero(), T::zero()),
        )
    }

    #[inline]
    fn min(&self) -> Self::Point {
        self.min
    }
    #[inline]
    fn max(&self) -> Self::Point {
        self.max
    }
}

impl<T: BaseNum> AxisAlignedBox<T> for Box3<T> {
    type Point = Point3<T>;
    type Vector = Vector3<T>;

    #[inline]
    fn new(min: Self::Point, max: Self::Point) -> Self {
        Self { min: min, max: max }
    }

    #[inline]
    fn is_valid(&self) -> bool {
        let size = self.size();
        size.x >= T::zero() && size.y >= T::zero() && size.z >= T::zero()
    }
    #[inline]
    fn is_empty(&self) -> bool {
        let size = self.size();
        size.x <= T::zero() && size.y <= T::zero() && size.z <= T::zero()
    }

    #[inline]
    fn zero() -> Self {
        Self::new(
            Point3::new(T::zero(), T::zero(), T::zero()),
            Point3::new(T::zero(), T::zero(), T::zero()),
        )
    }

    #[inline]
    fn min(&self) -> Self::Point {
        self.min
    }
    #[inline]
    fn max(&self) -> Self::Point {
        self.max
    }
}

impl<S: NumCast + Copy> Box2<S> {
    /// Component-wise casting to another type
    #[inline]
    pub fn cast<T: NumCast>(&self) -> Option<Box2<T>> {
        let min = match self.min.cast() {
            Some(field) => field,
            None => return None,
        };
        let max = match self.max.cast() {
            Some(field) => field,
            None => return None,
        };
        Some(Box2 { min, max })
    }
}

impl<S: NumCast + Copy> Box3<S> {
    /// Component-wise casting to another type
    #[inline]
    pub fn cast<T: NumCast>(&self) -> Option<Box3<T>> {
        let min = match self.min.cast() {
            Some(field) => field,
            None => return None,
        };
        let max = match self.max.cast() {
            Some(field) => field,
            None => return None,
        };
        Some(Box3 { min, max })
    }
}

impl<S: BaseFloat> AbsDiffEq for Box2<S> {
    type Epsilon = S::Epsilon;

    fn default_epsilon() -> Self::Epsilon {
        S::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.min.abs_diff_eq(&other.min, epsilon) && self.max.abs_diff_eq(&other.max, epsilon)
    }

    fn abs_diff_ne(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.min.abs_diff_ne(&other.min, epsilon) || self.max.abs_diff_ne(&other.max, epsilon)
    }
}

impl<S: BaseFloat> AbsDiffEq for Box3<S> {
    type Epsilon = S::Epsilon;

    fn default_epsilon() -> Self::Epsilon {
        S::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.min.abs_diff_eq(&other.min, epsilon) && self.max.abs_diff_eq(&other.max, epsilon)
    }

    fn abs_diff_ne(&self, other: &Self, epsilon: Self::Epsilon) -> bool {
        self.min.abs_diff_ne(&other.min, epsilon) || self.max.abs_diff_ne(&other.max, epsilon)
    }
}

impl<S: BaseFloat> UlpsEq for Box2<S> {
    fn default_max_ulps() -> u32 {
        S::default_max_ulps()
    }

    fn ulps_eq(&self, other: &Self, epsilon: Self::Epsilon, max_ulps: u32) -> bool {
        self.min.ulps_eq(&other.min, epsilon, max_ulps)
            && self.max.ulps_eq(&other.max, epsilon, max_ulps)
    }

    fn ulps_ne(&self, other: &Self, epsilon: Self::Epsilon, max_ulps: u32) -> bool {
        self.min.ulps_ne(&other.min, epsilon, max_ulps)
            || self.max.ulps_ne(&other.max, epsilon, max_ulps)
    }
}

impl<S: BaseFloat> UlpsEq for Box3<S> {
    fn default_max_ulps() -> u32 {
        S::default_max_ulps()
    }

    fn ulps_eq(&self, other: &Self, epsilon: Self::Epsilon, max_ulps: u32) -> bool {
        self.min.ulps_eq(&other.min, epsilon, max_ulps)
            && self.max.ulps_eq(&other.max, epsilon, max_ulps)
    }

    fn ulps_ne(&self, other: &Self, epsilon: Self::Epsilon, max_ulps: u32) -> bool {
        self.min.ulps_ne(&other.min, epsilon, max_ulps)
            || self.max.ulps_ne(&other.max, epsilon, max_ulps)
    }
}

#[cfg(feature = "quickcheck")]
use quickcheck::{Arbitrary, Gen};

#[cfg(feature = "quickcheck")]
impl<T: Arbitrary + BaseNum> Arbitrary for Box2<T> {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        let (x1, x2, x3, x4) = Arbitrary::arbitrary(g);
        Box2::new(Point2::new(x1, x2), Point2::new(x3, x4))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            (
                self.min.x.clone(),
                self.min.y.clone(),
                self.max.x.clone(),
                self.max.y.clone(),
            )
                .shrink()
                .map(|(x1, x2, x3, x4)| Box2::new(Point2::new(x1, x2), Point2::new(x3, x4))),
        )
    }
}

#[cfg(feature = "quickcheck")]
impl<T: Arbitrary + BaseNum> Arbitrary for Box3<T> {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        let (x1, x2, x3, x4, x5, x6) = Arbitrary::arbitrary(g);
        Box3::new(Point3::new(x1, x2, x3), Point3::new(x4, x5, x6))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            (
                self.min.x.clone(),
                self.min.y.clone(),
                self.min.z.clone(),
                self.max.x.clone(),
                self.max.y.clone(),
                self.max.z.clone(),
            )
                .shrink()
                .map(|(x1, x2, x3, x4, x5, x6)| {
                    Box3::new(Point3::new(x1, x2, x3), Point3::new(x4, x5, x6))
                }),
        )
    }
}

/// A macro for constructing `Box2` using various types of origin points and
/// using `Into::into`.
///
/// The syntax of this macro assumes a coordinate space where the increases in
/// X and Y coordinates correspond to the right and down direction, respectively.
///
/// # Examples
///
/// ```
/// use {cggeom::{box2, Box2, prelude::*}, cgmath::Point2};
///
/// let ref_box = Box2::new(Point2::new(1, 2), Point2::new(5, 10));
///
/// assert_eq!(ref_box, box2!{ min: [1, 2], max: [5, 10] });
/// assert_eq!(ref_box, box2!{ top_left: [1, 2], size: [4, 8] });
/// assert_eq!(ref_box, box2!{ top_right: [5, 2], size: [4, 8] });
/// assert_eq!(ref_box, box2!{ bottom_left: [1, 10], size: [4, 8] });
/// assert_eq!(ref_box, box2!{ bottom_right: [5, 10], size: [4, 8] });
///
/// let ref_point = Box2::new(Point2::new(1, 2), Point2::new(1, 2));
///
/// assert_eq!(ref_point, box2!{ point: [1, 2] });
/// ```
#[macro_export]
macro_rules! box2 {
    {
        point: $point:expr$(,)*
    } => {
        {
            let point: $crate::cgmath::Point2<_> = Into::into($point);
            <$crate::Box2<_> as $crate::AxisAlignedBox<_>>::new(point, point)
        }
    };

    {
        min: $min:expr,
        max: $max:expr$(,)*
    } => {
        <$crate::Box2<_> as $crate::AxisAlignedBox<_>>::new(
            Into::into($min),
            Into::into($max),
        )
    };

    {
        top_left: $origin:expr,
        size: $size:expr$(,)*
    } => {
        {
            let origin: $crate::cgmath::Point2<_> = Into::into($origin);
            let size: $crate::cgmath::Vector2<_> = Into::into($size);
            <$crate::Box2<_> as $crate::AxisAlignedBox<_>>::new(
                $crate::cgmath::Point2::new(origin.x, origin.y),
                $crate::cgmath::Point2::new(origin.x + size.x, origin.y + size.y),
            )
        }
    };

    {
        top_right: $origin:expr,
        size: $size:expr$(,)*
    } => {
        {
            let origin: $crate::cgmath::Point2<_> = Into::into($origin);
            let size: $crate::cgmath::Vector2<_> = Into::into($size);
            <$crate::Box2<_> as $crate::AxisAlignedBox<_>>::new(
                $crate::cgmath::Point2::new(origin.x - size.x, origin.y),
                $crate::cgmath::Point2::new(origin.x, origin.y + size.y),
            )
        }
    };

    {
        bottom_left: $origin:expr,
        size: $size:expr$(,)*
    } => {
        {
            let origin: $crate::cgmath::Point2<_> = Into::into($origin);
            let size: $crate::cgmath::Vector2<_> = Into::into($size);
            <$crate::Box2<_> as $crate::AxisAlignedBox<_>>::new(
                $crate::cgmath::Point2::new(origin.x, origin.y - size.y),
                $crate::cgmath::Point2::new(origin.x + size.x, origin.y),
            )
        }
    };

    {
        bottom_right: $origin:expr,
        size: $size:expr$(,)*
    } => {
        {
            let origin: $crate::cgmath::Point2<_> = Into::into($origin);
            let size: $crate::cgmath::Vector2<_> = Into::into($size);
            <$crate::Box2<_> as $crate::AxisAlignedBox<_>>::new(
                $crate::cgmath::Point2::new(origin.x - size.x, origin.y - size.y),
                $crate::cgmath::Point2::new(origin.x, origin.y),
            )
        }
    }
}
