use cgmath::{BaseFloat, Matrix3, Rad, Vector2};

/// An extension trait for [`cgmath::Matrix3`] that adds methods for
/// constructing 2D transformations.
///
/// `cgmath` is primarily built for 3D applications. For instance, there are
/// several creation methods that assume implicit semantics for the uses in such
/// applications: `Matrix4` represents a 3D homogeneous transformation matrix
/// and `Matrix3` a non-translating 3D transformation matrix.
/// This means that methods for 2D transformations, e.g.,
/// `from_nonuniform_scale`, cannot be added to `Matrix3` without breaking the
/// conventions (i.e., marking all of the 2D/3D operations as so), introducing
/// inconsistencies (i.e., marking only the new 2D operations as so), or
/// confusing users (i.e. `from_nonuniform_scale` could mean any of 2D and
/// 3D for `Matrix3`). Thus [they gave up] supporting such problematic
/// operations at all.
///
/// [they gave up] https://github.com/rustgd/cgmath/pull/469#issuecomment-436041377
pub trait Matrix3TwoDimExt<S>: Sized {
    /// Create a homogeneous transformation matrix from a translation vector.
    fn from_translation(v: Vector2<S>) -> Self;
    /// Create a homogeneous transformation matrix from a scale value.
    fn from_scale_2d(value: S) -> Self;
    /// Create a homogeneous transformation matrix from a set of scale values.
    fn from_nonuniform_scale_2d(x: S, y: S) -> Self;
    /// Create a homogeneous transformation matrix from a rotation.
    fn from_angle<A: Into<Rad<S>>>(theta: A) -> Self;
}

impl<S: BaseFloat> Matrix3TwoDimExt<S> for Matrix3<S> {
    #[inline]
    fn from_translation(v: Vector2<S>) -> Self {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        Self::new(
            S::one(), S::zero(), S::zero(),
            S::zero(), S::one(), S::zero(),
            v.x, v.y, S::one(),
        )
    }

    #[inline]
    fn from_scale_2d(value: S) -> Self {
        Self::from_nonuniform_scale_2d(value, value)
    }

    #[inline]
    fn from_nonuniform_scale_2d(x: S, y: S) -> Self {
        #[cfg_attr(rustfmt, rustfmt_skip)]
        Self::new(
            x, S::zero(), S::zero(),
            S::zero(), y, S::zero(),
            S::zero(), S::zero(), S::one(),
        )
    }

    #[inline]
    fn from_angle<A: Into<Rad<S>>>(theta: A) -> Self {
        Self::from_angle_z(theta)
    }
}
