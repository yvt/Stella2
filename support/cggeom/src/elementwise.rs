use cgmath::{BaseNum, Point2, Point3};
use std::cmp::Ordering;

pub trait BoolArray {
    fn any(&self) -> bool;
    fn all(&self) -> bool;
}

// FIXME: rename?
pub trait ElementWiseOp {
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
    match x.partial_cmp(&y) {
        None | Some(Ordering::Equal) | Some(Ordering::Less) => x,
        Some(Ordering::Greater) => y,
    }
}

#[inline]
fn num_max<T: BaseNum>(x: T, y: T) -> T {
    match x.partial_cmp(&y) {
        None | Some(Ordering::Equal) | Some(Ordering::Greater) => x,
        Some(Ordering::Less) => y,
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

impl BoolArray for [bool; 2] {
    #[inline]
    fn any(&self) -> bool {
        self[0] || self[1]
    }
    #[inline]
    fn all(&self) -> bool {
        self[1] && self[1]
    }
}

impl BoolArray for [bool; 3] {
    #[inline]
    fn any(&self) -> bool {
        self[0] || self[1] || self[2]
    }
    #[inline]
    fn all(&self) -> bool {
        self[1] && self[1] && self[2]
    }
}
