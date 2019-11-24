use cgmath::{BaseNum, Point2, Point3, Vector2, Vector3, Vector4};

pub trait BoolArray {
    fn any(&self) -> bool;
    fn all(&self) -> bool;
}

// FIXME: rename?
pub trait ElementWiseOp {
    // TODO: Rename these bois to like `fmin` and reimplement them based on
    //       `alt_fp::FloatOrd`
    fn element_wise_min(&self, rhs: &Self) -> Self;
    fn element_wise_max(&self, rhs: &Self) -> Self;
}

pub trait ElementWisePartialOrd {
    type Bool: BoolArray;
    fn element_wise_gt(&self, rhs: &Self) -> Self::Bool;
    fn element_wise_lt(&self, rhs: &Self) -> Self::Bool;
    fn element_wise_ge(&self, rhs: &Self) -> Self::Bool;
    fn element_wise_le(&self, rhs: &Self) -> Self::Bool;
}

#[inline]
fn num_min<T: BaseNum>(x: T, y: T) -> T {
    if y < x {
        y
    } else {
        x
    }
}

#[inline]
fn num_max<T: BaseNum>(x: T, y: T) -> T {
    if y > x {
        y
    } else {
        x
    }
}

impl<T: BaseNum> ElementWiseOp for Point2<T> {
    fn element_wise_min(&self, rhs: &Self) -> Self {
        Self::new(num_min(self.x, rhs.x), num_min(self.y, rhs.y))
    }
    fn element_wise_max(&self, rhs: &Self) -> Self {
        Self::new(num_max(self.x, rhs.x), num_max(self.y, rhs.y))
    }
}

impl<T: BaseNum> ElementWiseOp for Point3<T> {
    fn element_wise_min(&self, rhs: &Self) -> Self {
        Self::new(
            num_min(self.x, rhs.x),
            num_min(self.y, rhs.y),
            num_min(self.z, rhs.z),
        )
    }
    fn element_wise_max(&self, rhs: &Self) -> Self {
        Self::new(
            num_max(self.x, rhs.x),
            num_max(self.y, rhs.y),
            num_max(self.z, rhs.z),
        )
    }
}

impl<T: BaseNum> ElementWiseOp for Vector2<T> {
    fn element_wise_min(&self, rhs: &Self) -> Self {
        Self::new(num_min(self.x, rhs.x), num_min(self.y, rhs.y))
    }
    fn element_wise_max(&self, rhs: &Self) -> Self {
        Self::new(num_max(self.x, rhs.x), num_max(self.y, rhs.y))
    }
}

impl<T: BaseNum> ElementWiseOp for Vector3<T> {
    fn element_wise_min(&self, rhs: &Self) -> Self {
        Self::new(
            num_min(self.x, rhs.x),
            num_min(self.y, rhs.y),
            num_min(self.z, rhs.z),
        )
    }
    fn element_wise_max(&self, rhs: &Self) -> Self {
        Self::new(
            num_max(self.x, rhs.x),
            num_max(self.y, rhs.y),
            num_max(self.z, rhs.z),
        )
    }
}

impl<T: BaseNum> ElementWiseOp for Vector4<T> {
    fn element_wise_min(&self, rhs: &Self) -> Self {
        Self::new(
            num_min(self.x, rhs.x),
            num_min(self.y, rhs.y),
            num_min(self.z, rhs.z),
            num_min(self.w, rhs.w),
        )
    }
    fn element_wise_max(&self, rhs: &Self) -> Self {
        Self::new(
            num_max(self.x, rhs.x),
            num_max(self.y, rhs.y),
            num_max(self.z, rhs.z),
            num_max(self.w, rhs.w),
        )
    }
}

impl<T: PartialOrd> ElementWisePartialOrd for Point2<T> {
    type Bool = [bool; 2];
    fn element_wise_gt(&self, rhs: &Self) -> Self::Bool {
        [self.x > rhs.x, self.y > rhs.y]
    }
    fn element_wise_lt(&self, rhs: &Self) -> Self::Bool {
        [self.x < rhs.x, self.y < rhs.y]
    }
    fn element_wise_ge(&self, rhs: &Self) -> Self::Bool {
        [self.x >= rhs.x, self.y >= rhs.y]
    }
    fn element_wise_le(&self, rhs: &Self) -> Self::Bool {
        [self.x <= rhs.x, self.y <= rhs.y]
    }
}

impl<T: PartialOrd> ElementWisePartialOrd for Point3<T> {
    type Bool = [bool; 3];
    fn element_wise_gt(&self, rhs: &Self) -> Self::Bool {
        [self.x > rhs.x, self.y > rhs.y, self.z > rhs.z]
    }
    fn element_wise_lt(&self, rhs: &Self) -> Self::Bool {
        [self.x < rhs.x, self.y < rhs.y, self.z < rhs.z]
    }
    fn element_wise_ge(&self, rhs: &Self) -> Self::Bool {
        [self.x >= rhs.x, self.y >= rhs.y, self.z >= rhs.z]
    }
    fn element_wise_le(&self, rhs: &Self) -> Self::Bool {
        [self.x <= rhs.x, self.y <= rhs.y, self.z <= rhs.z]
    }
}

impl<T: PartialOrd> ElementWisePartialOrd for Vector2<T> {
    type Bool = [bool; 2];
    fn element_wise_gt(&self, rhs: &Self) -> Self::Bool {
        [self.x > rhs.x, self.y > rhs.y]
    }
    fn element_wise_lt(&self, rhs: &Self) -> Self::Bool {
        [self.x < rhs.x, self.y < rhs.y]
    }
    fn element_wise_ge(&self, rhs: &Self) -> Self::Bool {
        [self.x >= rhs.x, self.y >= rhs.y]
    }
    fn element_wise_le(&self, rhs: &Self) -> Self::Bool {
        [self.x <= rhs.x, self.y <= rhs.y]
    }
}

impl<T: PartialOrd> ElementWisePartialOrd for Vector3<T> {
    type Bool = [bool; 3];
    fn element_wise_gt(&self, rhs: &Self) -> Self::Bool {
        [self.x > rhs.x, self.y > rhs.y, self.z > rhs.z]
    }
    fn element_wise_lt(&self, rhs: &Self) -> Self::Bool {
        [self.x < rhs.x, self.y < rhs.y, self.z < rhs.z]
    }
    fn element_wise_ge(&self, rhs: &Self) -> Self::Bool {
        [self.x >= rhs.x, self.y >= rhs.y, self.z >= rhs.z]
    }
    fn element_wise_le(&self, rhs: &Self) -> Self::Bool {
        [self.x <= rhs.x, self.y <= rhs.y, self.z <= rhs.z]
    }
}

impl<T: PartialOrd> ElementWisePartialOrd for Vector4<T> {
    type Bool = [bool; 4];
    fn element_wise_gt(&self, rhs: &Self) -> Self::Bool {
        [
            self.x > rhs.x,
            self.y > rhs.y,
            self.z > rhs.z,
            self.w > rhs.w,
        ]
    }
    fn element_wise_lt(&self, rhs: &Self) -> Self::Bool {
        [
            self.x < rhs.x,
            self.y < rhs.y,
            self.z < rhs.z,
            self.w < rhs.w,
        ]
    }
    fn element_wise_ge(&self, rhs: &Self) -> Self::Bool {
        [
            self.x >= rhs.x,
            self.y >= rhs.y,
            self.z >= rhs.z,
            self.w >= rhs.w,
        ]
    }
    fn element_wise_le(&self, rhs: &Self) -> Self::Bool {
        [
            self.x <= rhs.x,
            self.y <= rhs.y,
            self.z <= rhs.z,
            self.w <= rhs.w,
        ]
    }
}

impl BoolArray for [bool; 2] {
    #[inline]
    fn any(&self) -> bool {
        self[0] || self[1]
    }
    #[inline]
    fn all(&self) -> bool {
        self[0] && self[1]
    }
}

impl BoolArray for [bool; 3] {
    #[inline]
    fn any(&self) -> bool {
        self[0] || self[1] || self[2]
    }
    #[inline]
    fn all(&self) -> bool {
        self[0] && self[1] && self[2]
    }
}

impl BoolArray for [bool; 4] {
    #[inline]
    fn any(&self) -> bool {
        self[0] || self[1] || self[2] || self[3]
    }
    #[inline]
    fn all(&self) -> bool {
        self[0] && self[1] && self[2] && self[3]
    }
}
