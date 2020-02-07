/// A trait for types supporting the calculation of the average of two values.
pub trait Average2 {
    /// Average two values.
    ///
    /// # Examples
    ///
    /// ```
    /// use cggeom::Average2;
    /// assert_eq!(0u32.average2(&1000), 500);
    /// assert_eq!(127i8.average2(&-128), -1);
    /// assert_eq!(40.0f32.average2(&100.0f32), 70.0f32);
    /// ```
    fn average2(&self, other: &Self) -> Self;
}

macro_rules! impl_float {
    ($($ty:ty),*) => {$(
        impl Average2 for $ty {
            fn average2(&self, other: &Self) -> Self {
                self + (other - self) * 0.5
            }
        }
    )*};
}

macro_rules! impl_int {
    ($($ty:ty),*) => {$(
        impl Average2 for $ty {
            fn average2(&self, other: &Self) -> Self {
                (self >> 1) + (other >> 1) + ((self & 1) + (other & 1) >> 1)
            }
        }
    )*};
}

impl_float!(f32, f64);
impl_int!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

macro_rules! impl_struct {
    ($ty:ty, {$($field:ident),*}) => {
        impl<T: Average2> Average2 for $ty {
            fn average2(&self, other: &Self) -> Self {
                Self {
                    $($field: self.$field.average2(&other.$field)),*
                }
            }
        }
    };
}

impl_struct!(cgmath::Vector1<T>, { x });
impl_struct!(cgmath::Vector2<T>, {x, y});
impl_struct!(cgmath::Vector3<T>, {x, y, z});
impl_struct!(cgmath::Vector4<T>, {x, y, z, w});
impl_struct!(cgmath::Point1<T>, { x });
impl_struct!(cgmath::Point2<T>, {x, y});
impl_struct!(cgmath::Point3<T>, {x, y, z});
