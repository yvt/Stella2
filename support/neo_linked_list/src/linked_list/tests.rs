//! **This module is mostly based on [`tests.rs`] from the Rust standard
//! library.**
//!
//! [`tests.rs`]: https://github.com/rust-lang/rust/blob/5a1d028d4c8fc15473dc10473c38df162daa7b41/src/liballoc/collections/linked_list/tests.rs

use super::*;

use std::thread;
use std::vec::Vec;

use rand::{thread_rng, RngCore};

fn list_from<T: Clone>(v: &[T]) -> LinkedList<T> {
    v.iter().cloned().collect()
}

pub fn check_links<T: ?Sized>(list: &LinkedList<T>) {
    unsafe {
        let mut len = 0;
        let mut last_ptr: Option<&Node<T>> = None;
        let mut node_ptr: &Node<T>;
        match list.head {
            None => {
                // tail node should also be None.
                assert!(list.tail.is_none());
                assert!(list.is_empty());
                return;
            }
            Some(hdr) => node_ptr = &*Node::from_hdr(hdr).as_ptr(),
        }
        loop {
            match (last_ptr, node_ptr.hdr.prev) {
                (None, None) => {}
                (None, _) => panic!("prev link for head"),
                (Some(p), Some(pptr)) => {
                    assert_eq!(
                        p as *const Node<T>,
                        Node::from_hdr(pptr).as_ptr() as *const Node<T>
                    );
                }
                _ => panic!("prev link is none, not good"),
            }
            match node_ptr.hdr.next {
                Some(next) => {
                    last_ptr = Some(node_ptr);
                    node_ptr = &*Node::from_hdr(next).as_ptr();
                    len += 1;
                }
                None => {
                    len += 1;
                    break;
                }
            }
        }

        // verify that the tail node points to the last node.
        let tail = list.tail.as_ref().expect("some tail node");
        assert_eq!(
            Node::from_hdr(*tail).as_ptr() as *const Node<T>,
            node_ptr as *const Node<T>
        );
        // check that len matches interior links.
        assert_eq!(len, list.len());
    }
}

#[test]
fn test_append() {
    // Empty to empty
    {
        let mut m = LinkedList::<i32>::new();
        let mut n = LinkedList::new();
        m.append(&mut n);
        check_links(&m);
        assert_eq!(m.len(), 0);
        assert_eq!(n.len(), 0);
    }
    // Non-empty to empty
    {
        let mut m = LinkedList::new();
        let mut n = LinkedList::new();
        n.push_back(2);
        m.append(&mut n);
        check_links(&m);
        assert_eq!(m.len(), 1);
        assert_eq!(m.pop_back(), Some(2));
        assert_eq!(n.len(), 0);
        check_links(&m);
    }
    // Empty to non-empty
    {
        let mut m = LinkedList::new();
        let mut n = LinkedList::new();
        m.push_back(2);
        m.append(&mut n);
        check_links(&m);
        assert_eq!(m.len(), 1);
        assert_eq!(m.pop_back(), Some(2));
        check_links(&m);
    }

    // Non-empty to non-empty
    let v = vec![1, 2, 3, 4, 5];
    let u = vec![9, 8, 1, 2, 3, 4, 5];
    let mut m = list_from(&v);
    let mut n = list_from(&u);
    m.append(&mut n);
    check_links(&m);
    let mut sum = v;
    sum.extend_from_slice(&u);
    assert_eq!(sum.len(), m.len());
    for elt in sum {
        assert_eq!(m.pop_front(), Some(elt))
    }
    assert_eq!(n.len(), 0);
    // Let's make sure it's working properly, since we
    // did some direct changes to private members.
    n.push_back(3);
    assert_eq!(n.len(), 1);
    assert_eq!(n.pop_front(), Some(3));
    check_links(&n);
}

#[test]
fn test_clone_from() {
    // Short cloned from long
    {
        let v = vec![1, 2, 3, 4, 5];
        let u = vec![8, 7, 6, 2, 3, 4, 5];
        let mut m = list_from(&v);
        let n = list_from(&u);
        m.clone_from(&n);
        check_links(&m);
        assert_eq!(m, n);
        for elt in u {
            assert_eq!(m.pop_front(), Some(elt))
        }
    }
    // Long cloned from short
    {
        let v = vec![1, 2, 3, 4, 5];
        let u = vec![6, 7, 8];
        let mut m = list_from(&v);
        let n = list_from(&u);
        m.clone_from(&n);
        check_links(&m);
        assert_eq!(m, n);
        for elt in u {
            assert_eq!(m.pop_front(), Some(elt))
        }
    }
    // Two equal length lists
    {
        let v = vec![1, 2, 3, 4, 5];
        let u = vec![9, 8, 1, 2, 3];
        let mut m = list_from(&v);
        let n = list_from(&u);
        m.clone_from(&n);
        check_links(&m);
        assert_eq!(m, n);
        for elt in u {
            assert_eq!(m.pop_front(), Some(elt))
        }
    }
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
#[cfg(not(miri))] // Miri does not support threads
fn test_send() {
    let n = list_from(&[1, 2, 3]);
    thread::spawn(move || {
        check_links(&n);
        let a: &[_] = &[&1, &2, &3];
        assert_eq!(a, &*n.iter().collect::<Vec<_>>());
    })
    .join()
    .ok()
    .unwrap();
}

#[test]
fn test_fuzz() {
    for _ in 0..25 {
        fuzz_test(3);
        fuzz_test(16);
        #[cfg(not(miri))] // Miri is too slow
        fuzz_test(189);
    }
}

fn fuzz_test(sz: i32) {
    let mut m: LinkedList<_> = LinkedList::new();
    let mut v = vec![];
    for i in 0..sz {
        check_links(&m);
        let r: u8 = thread_rng().next_u32() as u8;
        match r % 6 {
            0 => {
                m.pop_back();
                v.pop();
            }
            1 => {
                if !v.is_empty() {
                    m.pop_front();
                    v.remove(0);
                }
            }
            2 | 4 => {
                m.push_front(-i);
                v.insert(0, -i);
            }
            3 | 5 | _ => {
                m.push_back(i);
                v.push(i);
            }
        }
    }

    check_links(&m);

    let mut i = 0;
    for (a, &b) in m.into_iter().zip(&v) {
        i += 1;
        assert_eq!(a, b);
    }
    assert_eq!(i, v.len());
}

#[test]
fn test_fuzz_dyn() {
    for _ in 0..25 {
        fuzz_test_dyn(3);
        fuzz_test_dyn(16);
        #[cfg(not(miri))] // Miri is too slow
        fuzz_test_dyn(189);
    }
}

fn fuzz_test_dyn(sz: i32) {
    trait Get {
        fn get(&self) -> i32;
    }

    impl Get for i32 {
        fn get(&self) -> i32 {
            *self
        }
    }

    let mut m: LinkedList<dyn Get> = LinkedList::new();
    let mut v = vec![];
    for i in 0..sz {
        check_links(&m);
        let r: u8 = thread_rng().next_u32() as u8;
        match r % 6 {
            0 => {
                m.pop_back_node();
                v.pop();
            }
            1 => {
                if !v.is_empty() {
                    m.pop_front_node();
                    v.remove(0);
                }
            }
            2 | 4 => {
                m.push_front_node(Box::pin(Node::new(-i)));
                v.insert(0, -i);
            }
            3 | 5 | _ => {
                m.push_back_node(Box::pin(Node::new(i)));
                v.push(i);
            }
        }
    }

    check_links(&m);

    let m = std::iter::from_fn(|| m.pop_front_node().map(|g| g.element.get()));

    let mut i = 0;
    for (a, &b) in m.zip(&v) {
        i += 1;
        assert_eq!(a, b);
    }
    assert_eq!(i, v.len());
}
