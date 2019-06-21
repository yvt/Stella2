//! Offset traits and types
use std::ops::{Add, AddAssign, Neg};

/// A trait for types whose length can be calculated.
pub trait ToOffset<O: Offset> {
    fn to_offset(&self) -> O;
}

impl<T> ToOffset<isize> for Vec<T> {
    fn to_offset(&self) -> isize {
        assert!(self.len() <= <isize>::max_value() as usize, "len overflow");
        self.len() as isize
    }
}

impl ToOffset<isize> for String {
    fn to_offset(&self) -> isize {
        assert!(self.len() <= <isize>::max_value() as usize, "len overflow");
        self.len() as isize
    }
}

/// A trait for offset values.
///
/// Additive operations on offset values must be exact.
pub trait Offset: Neg<Output = Self> + Add<Output = Self> + AddAssign + Sized + Clone {
    fn zero() -> Self;
}

impl Offset for isize {
    fn zero() -> isize {
        0
    }
}

/// [`Offset`] having no information.
///
/// `ToOffset<NullOffset>` is automatically implemented for all types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NullOffset;

impl Add for NullOffset {
    type Output = Self;
    fn add(self, _: Self) -> Self {
        self
    }
}

impl AddAssign for NullOffset {
    fn add_assign(&mut self, _: Self) {}
}

impl Neg for NullOffset {
    type Output = Self;
    fn neg(self) -> Self {
        self
    }
}

impl Offset for NullOffset {
    fn zero() -> Self {
        NullOffset
    }
}

impl<T> ToOffset<NullOffset> for T {
    fn to_offset(&self) -> NullOffset {
        NullOffset
    }
}

/// Adds an element index to another [`Offset`].
///
/// [`ToOffset`]`<IndexOffset<T>>` is automatically implemented for types
/// that implement `ToOffset<T>`.
///
/// # Examples
///
/// ```
/// use rope::{IndexOffset, ToOffset};
/// let len: isize = "derp".to_string().to_offset();
/// assert_eq!(len, 4); // length
///
/// // `String` implements `ToOffset<isize>`, so it automatically
/// // implements `ToOffset<IndexOffset<isize>>`:
/// let len_with_count: IndexOffset<isize> = "derp".to_string().to_offset();
/// assert_eq!(len_with_count.0, 1); // element count - always `1`
/// assert_eq!(len_with_count.1, 4); // length (inherited)
///
/// // Usage with `Rope`:
/// use rope::Rope;
/// let rope: Rope<String, IndexOffset<isize>> = [
///     "Pony ", "ipsum ", "dolor ", "sit ", "amet ", "ms ",
/// ].iter().map(|x|x.to_string()).collect();
///
/// assert_eq!(rope.offset_len(), IndexOffset(6, 29));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOffset<T>(pub isize, pub T);

impl<T> Add for IndexOffset<T>
where
    T: Add<T, Output = T>,
{
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        IndexOffset(self.0 + rhs.0, self.1 + rhs.1)
    }
}

impl<T> AddAssign for IndexOffset<T>
where
    T: AddAssign,
{
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
        self.1 += rhs.1;
    }
}

impl<T> Neg for IndexOffset<T>
where
    T: Neg<Output = T>,
{
    type Output = Self;
    fn neg(self) -> Self {
        IndexOffset(-self.0, -self.1)
    }
}

impl<T> Offset for IndexOffset<T>
where
    T: Add<T, Output = T> + Neg<Output = T> + Offset,
{
    fn zero() -> Self {
        IndexOffset(0, T::zero())
    }
}

impl<T, O> ToOffset<IndexOffset<O>> for T
where
    T: ToOffset<O>,
    O: Offset,
{
    fn to_offset(&self) -> IndexOffset<O> {
        IndexOffset(1, ToOffset::<O>::to_offset(self))
    }
}

/// `Offset` representing an element index.
pub type Index = IndexOffset<NullOffset>;
