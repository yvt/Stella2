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

/// `RopeRangeBounds` based on a key extraction function and endpoints, which are
/// compared using `Ord::cmp`.
#[derive(Debug, Clone, Copy)]
pub struct RangeByKey<'a, KF, K> {
    pub extract_key: KF,
    pub start: Option<Edge<&'a K>>,
    pub end: Option<Edge<&'a K>>,
}

impl<O, KF, K> RopeRangeBounds<O> for RangeByKey<'_, KF, K>
where
    KF: FnMut(&O) -> K,
    K: Ord,
{
    fn start_ty(&self) -> Option<EdgeType> {
        self.start.as_ref().map(Edge::ty)
    }
    fn end_ty(&self) -> Option<EdgeType> {
        self.end.as_ref().map(Edge::ty)
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        let ep = self.start.as_ref().expect("unbounded").value_ref();
        (self.extract_key)(probe).cmp(ep)
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        let ep = self.end.as_ref().expect("unbounded").value_ref();
        (self.extract_key)(probe).cmp(ep)
    }
}

/// Construct a [`RangeByKey`] from a key extraction function and a range
/// implementing `std::ops::RangeBounds`.
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
/// let (iter, _) = rope.range(range_by_key(|i: &Index| i.0, &(Floor(1)..Floor(3))));
/// assert_eq!(
///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
///     &["ipsum ", "dolor "],
/// );
/// ```
pub fn range_by_key<KF, K>(
    extract_key: KF,
    range: &impl RangeBounds<Edge<K>>,
) -> RangeByKey<'_, KF, K> {
    RangeByKey {
        extract_key,
        start: match range.start_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.as_ref()),
            Bound::Unbounded => None,
        },
        end: match range.end_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.as_ref()),
            Bound::Unbounded => None,
        },
    }
}

/// `RopeRangeBounds` based on endpoints of type `O`, which are compared using
/// `O::cmp`.
#[derive(Debug, Clone, Copy)]
pub struct RangeByOrd<'a, O> {
    pub start: Option<Edge<&'a O>>,
    pub end: Option<Edge<&'a O>>,
}

impl<O> RopeRangeBounds<O> for RangeByOrd<'_, O>
where
    O: Ord,
{
    fn start_ty(&self) -> Option<EdgeType> {
        self.start.as_ref().map(Edge::ty)
    }
    fn end_ty(&self) -> Option<EdgeType> {
        self.end.as_ref().map(Edge::ty)
    }
    fn start_cmp(&mut self, probe: &O) -> Ordering {
        let ep = self.start.as_ref().expect("unbounded").value_ref();
        probe.cmp(ep)
    }
    fn end_cmp(&mut self, probe: &O) -> Ordering {
        let ep = self.end.as_ref().expect("unbounded").value_ref();
        probe.cmp(ep)
    }
}

/// Construct a [`RangeByOrd`] from a range implementing `std::ops::RangeBounds`.
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
/// let (iter, _) = rope.range(range_by_ord(&(Floor(7)..Floor(17))));
/// assert_eq!(
///     iter.map(String::as_str).collect::<Vec<_>>().as_slice(),
///     &["ipsum ", "dolor "],
/// );
/// ```
pub fn range_by_ord<O>(range: &impl RangeBounds<Edge<O>>) -> RangeByOrd<'_, O>
where
    O: Ord + Clone,
{
    RangeByOrd {
        start: match range.start_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.as_ref()),
            Bound::Unbounded => None,
        },
        end: match range.end_bound() {
            Bound::Included(x) | Bound::Excluded(x) => Some(x.as_ref()),
            Bound::Unbounded => None,
        },
    }
}
