use atom2::Atom;
use std::sync::{atomic::Ordering, Arc};

#[test]
fn arc_into_inner_some() {
    let aa = Atom::new(Some(Arc::new(1)));
    assert_eq!(*aa.into_inner().unwrap(), 1);
}

#[test]
fn arc_into_inner_none() {
    let aa: Atom<Arc<u32>> = Atom::empty();
    assert!(aa.into_inner().is_none());
}

#[test]
fn arc_as_inner_ref_some() {
    let mut aa = Atom::new(Some(Arc::new(1)));
    assert_eq!(*aa.as_inner_ref().unwrap(), 1);
}

#[test]
fn arc_as_inner_ref_none() {
    let mut aa: Atom<Arc<u32>> = Atom::empty();
    assert!(aa.as_inner_ref().is_none());
}

#[test]
fn box_as_inner_mut_some() {
    let mut aa = Atom::new(Some(Box::new(1)));
    assert_eq!(*aa.as_inner_mut().unwrap(), 1);
    *aa.as_inner_mut().unwrap() = 2;
    assert_eq!(*aa.into_inner().unwrap(), 2);
}

#[test]
fn box_as_inner_mut_none() {
    let mut aa: Atom<Box<u32>> = Atom::empty();
    assert!(aa.as_inner_mut().is_none());
}

#[test]
fn arc_load_some() {
    let mut aa = Atom::new(Some(Arc::new(1)));
    assert_eq!(*aa.load().unwrap(), 1);
}

#[test]
fn arc_load_none() {
    let mut aa: Atom<Arc<u32>> = Atom::empty();
    assert!(aa.load().is_none());
}

#[test]
fn arc_swap() {
    let aa = Atom::new(Some(Arc::new(1)));
    let old = aa.swap(Some(Arc::new(2)), Ordering::Relaxed);
    assert_eq!(*old.unwrap(), 1);
    assert_eq!(*aa.into_inner().unwrap(), 2);
}

#[test]
fn arc_compare_and_swap1() {
    let cur = Some(Arc::new(1));
    let aa = Atom::new(cur.clone());
    let old = aa.compare_and_swap(&cur, Some(Arc::new(2)), Ordering::Relaxed);
    assert_eq!(*old.unwrap().unwrap(), 1);
    assert_eq!(*aa.into_inner().unwrap(), 2);
}

#[test]
fn arc_compare_and_swap2() {
    let cur = Some(Arc::new(114514));
    let aa = Atom::new(Some(Arc::new(1)));
    let old = aa.compare_and_swap(&cur, Some(Arc::new(2)), Ordering::Relaxed);
    assert_eq!(*old.unwrap_err().unwrap(), 2);
    assert_eq!(*aa.into_inner().unwrap(), 1);
}
