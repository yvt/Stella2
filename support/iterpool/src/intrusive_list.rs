//! Intrusive doubly linked list for `Pool`s and `IterablePool`s.
use crate::PoolPtr;
use std::mem::transmute;
use std::ops;

/// Circualr linked list header.
#[derive(Debug, Default, Copy, Clone)]
pub struct ListHead {
    pub first: Option<PoolPtr>,
}

/// Links to neighbor items.
#[derive(Debug, Copy, Clone)]
pub struct Link {
    pub prev: PoolPtr,
    pub next: PoolPtr,
}

impl ListHead {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.first.is_none()
    }

    pub fn accessor<'a, P, F, T>(&'a self, pool: &'a P, field: F) -> ListAccessor<'a, P, F>
    where
        P: 'a + ops::Index<PoolPtr, Output = T>,
        F: Fn(&T) -> &Option<Link>,
    {
        ListAccessor {
            head: self,
            pool,
            field,
        }
    }

    pub fn accessor_mut<'a, P, F, T>(
        &'a mut self,
        pool: &'a mut P,
        field: F,
    ) -> ListAccessorMut<'a, P, F>
    where
        P: 'a + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
        F: FnMut(&mut T) -> &mut Option<Link>,
    {
        ListAccessorMut {
            head: self,
            pool,
            field,
        }
    }
}

/// Accessor to a linked list.
#[derive(Debug)]
pub struct ListAccessor<'a, P, F> {
    head: &'a ListHead,
    pool: &'a P,
    field: F,
}

impl<'a, P, F, T> ListAccessor<'a, P, F>
where
    P: ops::Index<PoolPtr, Output = T>,
    F: Fn(&T) -> &Option<Link>,
{
    pub fn head(&self) -> &ListHead {
        self.head
    }

    pub fn pool(&self) -> &P {
        self.pool
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_empty()
    }

    pub fn front(&self) -> Option<PoolPtr> {
        self.head.first
    }

    pub fn back(&self) -> Option<PoolPtr> {
        self.head
            .first
            .map(|p| (self.field)(&self.pool[p]).unwrap().prev)
    }

    pub fn front_data(&self) -> Option<&T> {
        if let Some(p) = self.front() {
            Some(&self.pool[p])
        } else {
            None
        }
    }

    pub fn back_data(&self) -> Option<&T> {
        if let Some(p) = self.back() {
            Some(&self.pool[p])
        } else {
            None
        }
    }

    pub fn iter(&self) -> Iter<&Self> {
        Iter {
            next: self.head.first,
            accessor: self,
        }
    }
}

impl<'a, P: 'a, F> ops::Deref for ListAccessor<'a, P, F> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

/// Mutable accessor to a linked list.
#[derive(Debug)]
pub struct ListAccessorMut<'a, P, F> {
    head: &'a mut ListHead,
    pool: &'a mut P,
    field: F,
}

impl<'a, P, F, T> ListAccessorMut<'a, P, F>
where
    P: ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: FnMut(&mut T) -> &mut Option<Link>,
{
    pub fn head(&self) -> &ListHead {
        self.head
    }

    pub fn head_mut(&mut self) -> &mut ListHead {
        self.head
    }

    pub fn pool(&self) -> &P {
        self.pool
    }

    pub fn pool_mut(&mut self) -> &mut P {
        self.pool
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_empty()
    }

    pub fn front(&mut self) -> Option<PoolPtr> {
        self.head.first
    }

    pub fn back(&mut self) -> Option<PoolPtr> {
        self.head
            .first
            .map(|p| (self.field)(&mut self.pool[p]).unwrap().prev)
    }

    pub fn front_data(&mut self) -> Option<&mut T> {
        if let Some(p) = self.front() {
            Some(&mut self.pool[p])
        } else {
            None
        }
    }

    pub fn back_data(&mut self) -> Option<&mut T> {
        if let Some(p) = self.back() {
            Some(&mut self.pool[p])
        } else {
            None
        }
    }

    /// Insert `item` before the position `p` (if `at` is `Some(p)`) or to the
    /// the list's back (if `at` is `None`).
    pub fn insert(&mut self, item: PoolPtr, at: Option<PoolPtr>) {
        #[allow(clippy::debug_assert_with_mut_call)]
        {
            debug_assert!(
                (self.field)(&mut self.pool[item]).is_none(),
                "item is already linked"
            );
        }

        if let Some(first) = self.head.first {
            let (next, update_first) = if let Some(at) = at {
                (at, at == first)
            } else {
                (first, false)
            };

            let prev = (self.field)(&mut self.pool[next]).unwrap().prev;
            (self.field)(&mut self.pool[prev]).as_mut().unwrap().next = item;
            (self.field)(&mut self.pool[next]).as_mut().unwrap().prev = item;
            *(self.field)(&mut self.pool[item]) = Some(Link { prev, next });

            if update_first {
                self.head.first = Some(item);
            }
        } else {
            debug_assert!(at.is_none());

            let link = (self.field)(&mut self.pool[item]);
            self.head.first = Some(item);
            *link = Some(Link {
                prev: item,
                next: item,
            });
        }
    }

    pub fn push_back(&mut self, item: PoolPtr) {
        self.insert(item, None);
    }

    pub fn push_front(&mut self, item: PoolPtr) {
        let at = self.front();
        self.insert(item, at);
    }

    /// Remove `item` from the list. Returns `item`.
    pub fn remove(&mut self, item: PoolPtr) -> PoolPtr {
        #[allow(clippy::debug_assert_with_mut_call)]
        {
            debug_assert!(
                (self.field)(&mut self.pool[item]).is_some(),
                "item is not linked"
            );
        }

        let link: Link = {
            let link_ref = (self.field)(&mut self.pool[item]);
            if self.head.first == Some(item) {
                let next = link_ref.unwrap().next;
                if next == item {
                    // The list just became empty
                    self.head.first = None;
                    *link_ref = None;
                    return item;
                }

                // Move the head pointer
                self.head.first = Some(next);
            }

            link_ref.unwrap()
        };

        (self.field)(&mut self.pool[link.prev])
            .as_mut()
            .unwrap()
            .next = link.next;
        (self.field)(&mut self.pool[link.next])
            .as_mut()
            .unwrap()
            .prev = link.prev;
        *(self.field)(&mut self.pool[item]) = None;

        item
    }

    pub fn pop_back(&mut self) -> Option<PoolPtr> {
        self.back().map(|item| self.remove(item))
    }

    pub fn pop_front(&mut self) -> Option<PoolPtr> {
        self.front().map(|item| self.remove(item))
    }

    /// Create an iterator.
    ///
    /// # Safety
    ///
    /// If the link structure is corrupt, it may return a mutable reference to
    /// the same element more than once, which is an undefined behavior.
    pub unsafe fn iter_mut(&mut self) -> Iter<&mut Self> {
        Iter {
            next: self.head.first,
            accessor: self,
        }
    }

    /// Create a draining iterator.
    ///
    /// # Safety
    ///
    /// If the link structure is corrupt, it may return a mutable reference to
    /// the same element more than once, which is an undefined behavior.
    pub unsafe fn drain<'b>(&'b mut self) -> Drain<'a, 'b, P, F, T> {
        Drain { accessor: self }
    }
}

impl<'a, P: 'a, F> ops::Deref for ListAccessorMut<'a, P, F> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl<'a, P: 'a, F> ops::DerefMut for ListAccessorMut<'a, P, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pool
    }
}

impl<'a, P, F, T> Extend<PoolPtr> for ListAccessorMut<'a, P, F>
where
    P: 'a + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: FnMut(&mut T) -> &mut Option<Link>,
{
    fn extend<I: IntoIterator<Item = PoolPtr>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item);
        }
    }
}

pub trait CellLike {
    type Target;

    fn get(&self) -> Self::Target;
    fn set(&self, value: Self::Target);

    fn modify(&self, f: impl FnOnce(&mut Self::Target))
    where
        Self: Sized,
    {
        let mut x = self.get();
        f(&mut x);
        self.set(x);
    }
}

impl<T: Copy> CellLike for std::cell::Cell<T> {
    type Target = T;

    fn get(&self) -> Self::Target {
        self.get()
    }
    fn set(&self, value: Self::Target) {
        self.set(value);
    }
}

impl<T: CellLike> CellLike for &T {
    type Target = T::Target;

    fn get(&self) -> Self::Target {
        (*self).get()
    }
    fn set(&self, value: Self::Target) {
        (*self).set(value);
    }
}

/// `Cell`-based accessor to a linked list.
#[derive(Debug)]
pub struct ListAccessorCell<'a, H, P, F> {
    head: H,
    pool: &'a P,
    field: F,
}

impl<'a, H, P, F, T, L> ListAccessorCell<'a, H, P, F>
where
    H: CellLike<Target = ListHead>,
    P: ops::Index<PoolPtr, Output = T>,
    F: Fn(&T) -> &L,
    L: CellLike<Target = Option<Link>>,
{
    pub fn new(head: H, pool: &'a P, field: F) -> Self {
        ListAccessorCell { head, pool, field }
    }

    pub fn head_cell(&self) -> &H {
        &self.head
    }

    pub fn head(&self) -> ListHead {
        self.head.get()
    }

    pub fn set_head(&self, head: ListHead) {
        self.head.set(head);
    }

    pub fn pool(&self) -> &P {
        self.pool
    }

    pub fn is_empty(&self) -> bool {
        self.head().is_empty()
    }

    pub fn front(&self) -> Option<PoolPtr> {
        self.head().first
    }

    pub fn back(&self) -> Option<PoolPtr> {
        self.head()
            .first
            .map(|p| (self.field)(&self.pool[p]).get().unwrap().prev)
    }

    pub fn front_data(&self) -> Option<&T> {
        if let Some(p) = self.front() {
            Some(&self.pool[p])
        } else {
            None
        }
    }

    pub fn back_data(&self) -> Option<&T> {
        if let Some(p) = self.back() {
            Some(&self.pool[p])
        } else {
            None
        }
    }

    /// Insert `item` before the position `p` (if `at` is `Some(p)`) or to the
    /// the list's back (if `at` is `None`).
    pub fn insert(&self, item: PoolPtr, at: Option<PoolPtr>) {
        debug_assert!(
            (self.field)(&self.pool[item]).get().is_none(),
            "item is already linked"
        );

        let mut head = self.head();

        if let Some(first) = head.first {
            let (next, update_first) = if let Some(at) = at {
                (at, at == first)
            } else {
                (first, false)
            };

            let prev = (self.field)(&self.pool[next]).get().unwrap().prev;
            (self.field)(&self.pool[prev]).modify(|l| l.as_mut().unwrap().next = item);
            (self.field)(&self.pool[next]).modify(|l| l.as_mut().unwrap().prev = item);
            (self.field)(&self.pool[item]).set(Some(Link { prev, next }));

            if update_first {
                head.first = Some(item);
                self.set_head(head);
            }
        } else {
            debug_assert!(at.is_none());

            let link = (self.field)(&self.pool[item]);
            link.set(Some(Link {
                prev: item,
                next: item,
            }));

            head.first = Some(item);
            self.set_head(head);
        }
    }

    pub fn push_back(&self, item: PoolPtr) {
        self.insert(item, None);
    }

    pub fn push_front(&self, item: PoolPtr) {
        let at = self.front();
        self.insert(item, at);
    }

    /// Remove `item` from the list. Returns `item`.
    pub fn remove(&self, item: PoolPtr) -> PoolPtr {
        debug_assert!(
            (self.field)(&self.pool[item]).get().is_some(),
            "item is not linked"
        );

        let link: Link = {
            let link_ref = (self.field)(&self.pool[item]);
            let mut head = self.head();
            if head.first == Some(item) {
                let next = link_ref.get().unwrap().next;
                if next == item {
                    // The list just became empty
                    head.first = None;
                    self.set_head(head);

                    link_ref.set(None);
                    return item;
                }

                // Move the head pointer
                head.first = Some(next);
                self.set_head(head);
            }

            link_ref.get().unwrap()
        };

        (self.field)(&self.pool[link.prev]).modify(|l| l.as_mut().unwrap().next = link.next);
        (self.field)(&self.pool[link.next]).modify(|l| l.as_mut().unwrap().prev = link.prev);
        (self.field)(&self.pool[item]).set(None);

        item
    }

    pub fn pop_back(&self) -> Option<PoolPtr> {
        self.back().map(|item| self.remove(item))
    }

    pub fn pop_front(&self) -> Option<PoolPtr> {
        self.front().map(|item| self.remove(item))
    }

    pub fn iter(&self) -> Iter<&Self> {
        Iter {
            next: self.head().first,
            accessor: self,
        }
    }
}

impl<'a, H, P, F> ops::Deref for ListAccessorCell<'a, H, P, F> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        self.pool
    }
}

impl<'a, H, P, F, T, L> Extend<PoolPtr> for ListAccessorCell<'a, H, P, F>
where
    H: CellLike<Target = ListHead>,
    P: ops::Index<PoolPtr, Output = T>,
    F: Fn(&T) -> &L,
    L: CellLike<Target = Option<Link>>,
{
    fn extend<I: IntoIterator<Item = PoolPtr>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item);
        }
    }
}

/// An iterator over the elements of `ListAccessor`, `ListAccessorMut`, or
/// `ListAccessorCell`.
#[derive(Debug)]
pub struct Iter<T> {
    accessor: T,
    next: Option<PoolPtr>,
}

impl<'a, 'b, P, F, T> Iterator for Iter<&'b ListAccessor<'a, P, F>>
where
    P: ops::Index<PoolPtr, Output = T>,
    F: 'a + Fn(&T) -> &Option<Link>,
    T: 'a,
{
    type Item = (PoolPtr, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.next {
            let new_next = (self.accessor.field)(&self.accessor.pool[next])
                .unwrap()
                .next;
            if Some(new_next) == self.accessor.head.first {
                self.next = None;
            } else {
                self.next = Some(new_next);
            }
            Some((next, &self.accessor.pool[next]))
        } else {
            None
        }
    }
}

impl<'a, 'b, P, F, T> Iterator for Iter<&'b mut ListAccessorMut<'a, P, F>>
where
    P: ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
{
    type Item = (PoolPtr, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.next {
            let new_next = (self.accessor.field)(&mut self.accessor.pool[next])
                .unwrap()
                .next;
            if Some(new_next) == self.accessor.head.first {
                self.next = None;
            } else {
                self.next = Some(new_next);
            }
            Some((next, unsafe { transmute(&mut self.accessor.pool[next]) }))
        } else {
            None
        }
    }
}

impl<'a, 'b, H, P, F, T, L> Iterator for Iter<&'b ListAccessorCell<'a, H, P, F>>
where
    H: CellLike<Target = ListHead>,
    P: ops::Index<PoolPtr, Output = T>,
    F: 'a + Fn(&T) -> &L,
    T: 'a + 'b,
    L: CellLike<Target = Option<Link>>,
{
    type Item = (PoolPtr, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.next {
            let new_next = (self.accessor.field)(&self.accessor.pool[next])
                .get()
                .unwrap()
                .next;
            if Some(new_next) == self.accessor.head().first {
                self.next = None;
            } else {
                self.next = Some(new_next);
            }
            Some((next, &self.accessor.pool[next]))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct Drain<'a, 'b, P, F, T>
where
    P: ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
{
    accessor: &'b mut ListAccessorMut<'a, P, F>,
}

impl<'a, 'b, P, F, T> Iterator for Drain<'a, 'b, P, F, T>
where
    P: ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
{
    type Item = (PoolPtr, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = self.accessor.pop_front();
        if let Some(p) = ptr {
            Some((p, unsafe { transmute(&mut self.accessor.pool[p]) }))
        } else {
            None
        }
    }
}

impl<'a, 'b, P, F, T> Drop for Drain<'a, 'b, P, F, T>
where
    P: ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
{
    fn drop(&mut self) {
        while let Some(_) = self.accessor.pop_back() {}
    }
}

#[test]
fn basic_mut() {
    use crate::Pool;
    let mut pool = Pool::new();
    let mut head = ListHead::new();
    let mut accessor = head.accessor_mut(&mut pool, |&mut (_, ref mut link)| link);

    let ptr1 = accessor.allocate((1, None));
    accessor.push_back(ptr1);

    let ptr2 = accessor.allocate((2, None));
    accessor.push_back(ptr2);

    let ptr3 = accessor.allocate((3, None));
    accessor.push_front(ptr3);

    println!("{:?}", (accessor.pool(), accessor.head()));

    assert!(!accessor.is_empty());
    assert_eq!(accessor.front(), Some(ptr3));
    assert_eq!(accessor.back(), Some(ptr2));
    assert_eq!(accessor.front_data().unwrap().0, 3);
    assert_eq!(accessor.back_data().unwrap().0, 2);

    let items: Vec<_> = unsafe { accessor.iter_mut() }
        .map(|(_, &mut (x, _))| x)
        .collect();
    assert_eq!(items, vec![3, 1, 2]);

    accessor.remove(ptr1);
    accessor.remove(ptr2);
    accessor.remove(ptr3);

    assert!(accessor.is_empty());
}

#[test]
fn basic_cell() {
    use crate::Pool;
    use std::cell::Cell;
    let mut pool = Pool::new();
    let head = Cell::new(ListHead::new());

    macro_rules! get_accessor {
        () => {
            ListAccessorCell::new(&head, &pool, |(_, link)| link)
        };
    }

    let ptr1 = pool.allocate((1, Cell::new(None)));
    get_accessor!().push_back(ptr1);

    let ptr2 = pool.allocate((2, Cell::new(None)));
    get_accessor!().push_back(ptr2);

    let ptr3 = pool.allocate((3, Cell::new(None)));
    get_accessor!().push_front(ptr3);

    println!("{:?}", (&pool, &head));

    let accessor = get_accessor!();
    assert!(!accessor.is_empty());
    assert_eq!(accessor.front(), Some(ptr3));
    assert_eq!(accessor.back(), Some(ptr2));
    assert_eq!(accessor.front_data().unwrap().0, 3);
    assert_eq!(accessor.back_data().unwrap().0, 2);

    let items: Vec<_> = accessor.iter().map(|(_, (x, _))| *x).collect();
    assert_eq!(items, vec![3, 1, 2]);

    accessor.remove(ptr1);
    println!("{:?}", (&pool, &head));
    accessor.remove(ptr2);
    println!("{:?}", (&pool, &head));
    accessor.remove(ptr3);
    println!("{:?}", (&pool, &head));

    assert!(accessor.is_empty());
}

#[test]
fn drain() {
    use crate::Pool;
    let mut pool = Pool::new();
    let mut head = ListHead::new();
    let mut accessor = head.accessor_mut(&mut pool, |&mut (_, ref mut link)| link);

    let ptr1 = accessor.allocate((1, None));
    accessor.push_back(ptr1);

    let ptr2 = accessor.allocate((2, None));
    accessor.push_back(ptr2);

    let ptr3 = accessor.allocate((3, None));
    accessor.push_front(ptr3);

    let items: Vec<_> = unsafe { accessor.drain() }
        .map(|(_, &mut (x, _))| x)
        .collect();
    assert_eq!(items, vec![3, 1, 2]);

    assert!(accessor.is_empty());
}

#[test]
fn basic() {
    use crate::Pool;
    let mut pool = Pool::new();
    let mut head = ListHead::new();
    let (_, ptr2, ptr3) = {
        let mut accessor = head.accessor_mut(&mut pool, |&mut (_, ref mut link)| link);

        let ptr1 = accessor.allocate((1, None));
        accessor.push_back(ptr1);

        let ptr2 = accessor.allocate((2, None));
        accessor.push_back(ptr2);

        let ptr3 = accessor.allocate((3, None));
        accessor.push_front(ptr3);

        println!("{:?}", (accessor.pool(), accessor.head()));

        (ptr1, ptr2, ptr3)
    };

    let accessor = head.accessor(&pool, |&(_, ref link)| link);
    assert!(!accessor.is_empty());
    assert_eq!(accessor.front(), Some(ptr3));
    assert_eq!(accessor.back(), Some(ptr2));
    assert_eq!(accessor.front_data().unwrap().0, 3);
    assert_eq!(accessor.back_data().unwrap().0, 2);

    let items: Vec<_> = accessor.iter().map(|(_, &(x, _))| x).collect();
    assert_eq!(items, vec![3, 1, 2]);
}
