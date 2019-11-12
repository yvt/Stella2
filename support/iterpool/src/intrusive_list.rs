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
pub struct ListAccessor<'a, P: 'a, F> {
    head: &'a ListHead,
    pool: &'a P,
    field: F,
}

impl<'a, P, F, T> ListAccessor<'a, P, F>
where
    P: 'a + ops::Index<PoolPtr, Output = T>,
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
pub struct ListAccessorMut<'a, P: 'a, F> {
    head: &'a mut ListHead,
    pool: &'a mut P,
    field: F,
}

impl<'a, P, F, T> ListAccessorMut<'a, P, F>
where
    P: 'a + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
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

    pub fn iter_mut(&mut self) -> Iter<&mut Self> {
        Iter {
            next: self.head.first,
            accessor: self,
        }
    }

    pub fn drain<'b>(&'b mut self) -> Drain<'a, 'b, P, F, T> {
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

/// An iterator over the elements of `ListAccessor` or `ListAccessorMut`.
#[derive(Debug)]
pub struct Iter<T> {
    accessor: T,
    next: Option<PoolPtr>,
}

impl<'a, 'b, P, F, T> Iterator for Iter<&'b ListAccessor<'a, P, F>>
where
    P: 'a + 'b + ops::Index<PoolPtr, Output = T>,
    F: 'a + 'b + Fn(&T) -> &Option<Link>,
    T: 'a,
    'a: 'b,
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
    P: 'a + 'b + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + 'b + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
    'a: 'b,
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

#[derive(Debug)]
pub struct Drain<'a, 'b, P, F, T>
where
    P: 'a + 'b + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + 'b + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
    'a: 'b,
{
    accessor: &'b mut ListAccessorMut<'a, P, F>,
}

impl<'a, 'b, P, F, T> Iterator for Drain<'a, 'b, P, F, T>
where
    P: 'a + 'b + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + 'b + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
    'a: 'b,
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
    P: 'a + 'b + ops::Index<PoolPtr, Output = T> + ops::IndexMut<PoolPtr>,
    F: 'a + 'b + FnMut(&mut T) -> &mut Option<Link>,
    T: 'a + 'b,
    'a: 'b,
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

    let items: Vec<_> = accessor.iter_mut().map(|(_, &mut (x, _))| x).collect();
    assert_eq!(items, vec![3, 1, 2]);

    accessor.remove(ptr1);
    accessor.remove(ptr2);
    accessor.remove(ptr3);

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

    let items: Vec<_> = accessor.drain().map(|(_, &mut (x, _))| x).collect();
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
