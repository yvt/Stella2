//! Selectors - types used to select an element or a range.
use std::{
    cmp::Ordering,
    ops::{Bound, RangeBounds, RangeFrom, RangeFull, RangeTo},
};

/// Represents an endpoint for a range selection.
///
/// `Floor` and `Ceil` specify the direction to which an endpoint falling in
/// the middle of an element should be "rounded".
///
/// The inner value represents an endpoint position in one of the following
/// ways depending on where `Edge` is used:
///
///  - Implicit: `impl FnMut(&O) -> std::cmp::Ordering`
///  - Explicit: `K`
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge<T> {
    Floor(T),
    Ceil(T),
}

impl<T> Edge<T> {
    /// Convert `Edge<T>` to `Edge<&T>`.
    pub fn as_ref(&self) -> Edge<&T> {
        match self {
            Edge::Floor(x) => Edge::Floor(x),
            Edge::Ceil(x) => Edge::Ceil(x),
        }
    }

    /// Convert `Edge<T>` to `Edge<&mut T>`.
    pub fn as_mut(&mut self) -> Edge<&T> {
        match self {
            Edge::Floor(x) => Edge::Floor(x),
            Edge::Ceil(x) => Edge::Ceil(x),
        }
    }

    /// Get a reference to the inner value.
    pub fn value_ref(&self) -> &T {
        match self {
            Edge::Floor(x) => x,
            Edge::Ceil(x) => x,
        }
    }

    /// Get a mutable reference to the inner value.
    pub fn value_mut(&mut self) -> &mut T {
        match self {
            Edge::Floor(x) => x,
            Edge::Ceil(x) => x,
        }
    }

    /// Get the `EdgeType` of a `Edge`.
    pub fn ty(&self) -> EdgeType {
        match self {
            Edge::Floor(_) => EdgeType::Floor,
            Edge::Ceil(_) => EdgeType::Ceil,
        }
    }
}

/// The kind of [`Edge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeType {
    Floor,
    Ceil,
}

/// A trait for types specifying a range of elements in a [`Rope`].
///
/// See [`Rope::range`] for examples.
///
/// [`Rope`]: crate::Rope
/// [`Rope::range`]: crate::Rope::range
pub trait RopeRangeBounds<O: ?Sized> {
    /// Get `EdgeType` of the first endpoint. Returns `None` if unbounded.
    fn start_ty(&self) -> Option<EdgeType>;
    /// Get `EdgeType` of the second endpoint. Returns `None` if unbounded.
    fn end_ty(&self) -> Option<EdgeType>;
    /// Compare a value to the first endpoint. `start_ty()` must be `Some(_)`.
    fn start_cmp(&mut self, probe: &O) -> Ordering;
    /// Compare a value to the second endpoint. `end_ty()` must be `Some(_)`.
    fn end_cmp(&mut self, probe: &O) -> Ordering;
}

impl<T, O> RopeRangeBounds<O> for &'_ mut T
where
    T: ?Sized + RopeRangeBounds<O>,
    O: ?Sized,
{
    fn start_ty(&self) -> Option<EdgeType> {
        (**self).start_ty()
    }
    fn end_ty(&self) -> Option<EdgeType> {
        (**self).end_ty()
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        (**self).start_cmp(probe)
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        (**self).end_cmp(probe)
    }
}

impl<T, O> RopeRangeBounds<O> for Box<T>
where
    T: ?Sized + RopeRangeBounds<O>,
    O: ?Sized,
{
    fn start_ty(&self) -> Option<EdgeType> {
        (**self).start_ty()
    }
    fn end_ty(&self) -> Option<EdgeType> {
        (**self).end_ty()
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        (**self).start_cmp(probe)
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        (**self).end_cmp(probe)
    }
}

/// A bounded range.
///
/// It's hardly useful to implement `RopeRangeBounds` on `Range<F>` because
/// `Range<F>` requires both endpoints to have an identical type `F`.
impl<O, F1, F2> RopeRangeBounds<O> for (Edge<F1>, Edge<F2>)
where
    F1: FnMut(&O) -> Ordering,
    F2: FnMut(&O) -> Ordering,
    O: ?Sized,
{
    fn start_ty(&self) -> Option<EdgeType> {
        Some(self.0.ty())
    }
    fn end_ty(&self) -> Option<EdgeType> {
        Some(self.1.ty())
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        self.0.value_mut()(probe)
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        self.1.value_mut()(probe)
    }
}

impl<O, F> RopeRangeBounds<O> for RangeFrom<Edge<F>>
where
    F: FnMut(&O) -> Ordering,
    O: ?Sized,
{
    fn start_ty(&self) -> Option<EdgeType> {
        Some(self.start.ty())
    }
    fn end_ty(&self) -> Option<EdgeType> {
        None
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        self.start.value_mut()(probe)
    }
    fn end_cmp(&mut self, _: &O) -> Ordering {
        panic!("unbounded")
    }
}

impl<O, F> RopeRangeBounds<O> for RangeTo<Edge<F>>
where
    F: FnMut(&O) -> Ordering,
    O: ?Sized,
{
    fn start_ty(&self) -> Option<EdgeType> {
        None
    }
    fn end_ty(&self) -> Option<EdgeType> {
        Some(self.end.ty())
    }
    fn start_cmp(&mut self, _: &O) -> Ordering {
        panic!("unbounded")
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        self.end.value_mut()(probe)
    }
}

impl<O> RopeRangeBounds<O> for RangeFull
where
    O: ?Sized,
{
    fn start_ty(&self) -> Option<EdgeType> {
        None
    }
    fn end_ty(&self) -> Option<EdgeType> {
        None
    }
    fn start_cmp(&mut self, _: &O) -> Ordering {
        panic!("unbounded")
    }
    fn end_cmp(&mut self, _: &O) -> Ordering {
        panic!("unbounded")
    }
}

/// `RopeRangeBounds<O>` based on a key extraction function (which converts
/// from `O` to `K`) and  `RangeBounds<Edge<K>>`, which are compared against
/// keys using `K::cmp`.
///
/// For `range.start_bound()` and `range.end_bound()`, only the value inside the
/// `Included(x)` and `Excluded(x)` is considered.
/// (E.g., `Floor(4)..Floor(6)` and `Floor(4)..=Floor(6)` are treated
/// identically.)
///
/// # Examples
///
/// ```
/// use rope::{Rope, range_by_key, Index, Edge::Floor};
/// let rope: Rope<String, Index> = [
///     "Pony ", "ipsum ", "dolor ", "sit ", "amet ", "ms ",
/// ].iter().map(|x|x.to_string()).collect();
///
/// // Extract indices from `Index` and use them as key
/// let (iter, _) = rope.range(range_by_key(|i: &Index| i.0, Floor(1)..Floor(3)));
/// assert_eq!(
///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
///     &["ipsum ", "dolor "],
/// );
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RangeByKey<KF, R> {
    pub extract_key: KF,
    pub range: R,
}

impl<O, KF, K, R> RopeRangeBounds<O> for RangeByKey<KF, R>
where
    KF: FnMut(&O) -> K,
    K: Ord,
    R: RangeBounds<Edge<K>>,
{
    fn start_ty(&self) -> Option<EdgeType> {
        match self.range.start_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.ty()),
            Bound::Unbounded => None,
        }
    }
    fn end_ty(&self) -> Option<EdgeType> {
        match self.range.end_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.ty()),
            Bound::Unbounded => None,
        }
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        let ep = match self.range.start_bound() {
            Bound::Included(x) | Bound::Excluded(x) => x,
            Bound::Unbounded => panic!("unbounded"),
        };
        (self.extract_key)(probe).cmp(ep.value_ref())
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        let ep = match self.range.end_bound() {
            Bound::Included(x) | Bound::Excluded(x) => x,
            Bound::Unbounded => panic!("unbounded"),
        };
        (self.extract_key)(probe).cmp(ep.value_ref())
    }
}

/// A shorthand function for constructing `RangeByKey`.
pub fn range_by_key<KF, R>(extract_key: KF, range: R) -> RangeByKey<KF, R> {
    RangeByKey { extract_key, range }
}

/// `RopeRangeBounds<O>` based on `RangeBounds<Edge<O>>`. Endpoints are
/// compared using `O::cmp`.
///
/// For `range.start_bound()` and `range.end_bound()`, only the value inside the
/// `Included(x)` and `Excluded(x)` is considered.
/// (E.g., `Floor(4)..Floor(6)` and `Floor(4)..=Floor(6)` are treated
/// identically.)
///
/// # Examples
///
/// ```
/// use rope::{Rope, range_by_ord, Edge::Floor};
/// let rope: Rope<String> = [
///     "Pony ", "ipsum ", "dolor ", "sit ", "amet ", "ms ",
/// ].iter().map(|x|x.to_string()).collect();
///
/// let (iter, _) = rope.range(range_by_ord(Floor(7)..Floor(17)));
/// assert_eq!(
///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
///     &["ipsum ", "dolor "],
/// );
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RangeByOrd<R> {
    pub range: R,
}

impl<O, R> RopeRangeBounds<O> for RangeByOrd<R>
where
    O: Ord,
    R: RangeBounds<Edge<O>>,
{
    fn start_ty(&self) -> Option<EdgeType> {
        match self.range.start_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.ty()),
            Bound::Unbounded => None,
        }
    }
    fn end_ty(&self) -> Option<EdgeType> {
        match self.range.end_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.ty()),
            Bound::Unbounded => None,
        }
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        let ep = match self.range.start_bound() {
            Bound::Included(x) | Bound::Excluded(x) => x,
            Bound::Unbounded => panic!("unbounded"),
        };
        probe.cmp(ep.value_ref())
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        let ep = match self.range.end_bound() {
            Bound::Included(x) | Bound::Excluded(x) => x,
            Bound::Unbounded => panic!("unbounded"),
        };
        probe.cmp(ep.value_ref())
    }
}

/// A shorthand function for constructing `RangeByOrd`.
pub fn range_by_ord<R>(range: R) -> RangeByOrd<R> {
    RangeByOrd { range }
}

/// Specifies one element using a reference value (usually represented by a
/// comparator function implementing `FnMut(&O) -> Ordering`).
///
/// It specifies an element using in form "the first/last element after/before
/// a reference value". Note that it assumes that there's an infinitely large
/// empty place next to each endpoint of the rope, and it represents no elements
/// (thus APIs using `One` does nothing and/or return `None`) if it points one
/// of those empty places.
///
/// [`by_key`] and [`by_ord`] are high-order functions returning comparator
/// functions.
///
/// See [`Rope::get_with_offset`] for examples.
///
/// [`Rope::get_with_offset`]: crate::Rope::get_with_offset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum One<T> {
    /// The first element after a reference value.
    FirstAfter(T),
    /// The last element before a reference value.
    LastBefore(T),
}

/// Create a comparison function from a key extraction function and a
/// reference value.
///
/// # Examples
///
/// ```
/// use rope::{Rope, by_key, Index, One::FirstAfter};
/// let rope: Rope<String, Index> = [
///     "Pony ", "ipsum ", "dolor ", "sit ", "amet ", "ms ",
/// ].iter().map(|x|x.to_string()).collect();
///
/// // Extract indices from `Index` and use them as key
/// let elem = rope.get(FirstAfter(by_key(|i: &Index| i.0, 1))).unwrap();
/// assert_eq!(elem, "ipsum ");
/// ```
pub fn by_key<KF, K, O>(extract_key: KF, x: K) -> impl Fn(&O) -> Ordering
where
    KF: Fn(&O) -> K,
    K: Ord,
{
    move |probe| extract_key(probe).cmp(&x)
}

/// Create a comparison function from a reference value.
pub fn by_ord<O>(x: O) -> impl Fn(&O) -> Ordering
where
    O: Ord,
{
    move |probe| probe.cmp(&x)
}
