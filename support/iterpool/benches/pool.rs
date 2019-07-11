#![feature(test)]
extern crate iterpool;
extern crate test;
use test::Bencher;

use iterpool::*;

struct Xorshift32(u32);

impl Xorshift32 {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

#[bench]
fn allocation_random_std(b: &mut Bencher) {
    let mut v = vec![None; 512];
    b.iter(|| {
        let mut r = Xorshift32(0x11451419);
        for _ in 0..65536 {
            let i = ((r.next() >> 8) & 511) as usize;
            if v[i].is_some() {
                v[i] = None;
            } else {
                v[i] = Some(Box::new(i));
            }
        }
        let mut sum = 0;
        for x in v.iter_mut() {
            if let Some(x) = x.take() {
                sum += *x;
            }
        }
        sum
    });
}

#[bench]
fn allocation_random_pool(b: &mut Bencher) {
    let mut v = vec![None; 512];
    let mut pool = Pool::with_capacity(512);
    b.iter(|| {
        let mut r = Xorshift32(0x11451419);
        for _ in 0..65536 {
            let i = ((r.next() >> 8) & 511) as usize;
            if v[i].is_some() {
                pool.deallocate(v[i].take().unwrap());
            } else {
                v[i] = Some(pool.allocate(i));
            }
        }
        let mut sum = 0;
        for x in v.iter_mut() {
            if let Some(x) = x.take() {
                sum += pool[x];
                pool.deallocate(x);
            }
        }
        sum
    });
}

#[bench]
fn allocation_random_mt_std(b: &mut Bencher) {
    use std::thread::Builder;
    let mut states = vec![Some(vec![None; 512]); 8];
    b.iter(|| {
        let mut threads: Vec<_> = states
            .iter_mut()
            .map(|s| {
                let mut v = s.take().unwrap();
                Builder::new()
                    .spawn(move || {
                        let mut r = Xorshift32(0x11451419);
                        for _ in 0..655360 {
                            let i = ((r.next() >> 8) & 511) as usize;
                            if v[i].is_some() {
                                v[i] = None;
                            } else {
                                v[i] = Some(Box::new(i));
                            }
                        }
                        let mut sum = 0;
                        for x in v.iter_mut() {
                            if let Some(x) = x.take() {
                                sum += *x;
                            }
                        }
                        (v, sum)
                    })
                    .expect("failed to create thread")
            })
            .collect();
        let mut sum = 0;
        for (i, handle) in threads.drain(..).enumerate() {
            let (st, sub_sum) = handle.join().unwrap();
            states[i] = Some(st);
            sum += sub_sum;
        }
        sum
    });
}

#[bench]
fn allocation_random_mt_pool(b: &mut Bencher) {
    use std::thread::Builder;
    let mut states = vec![Some((vec![None; 512], Pool::with_capacity(512))); 8];
    b.iter(|| {
        let mut threads: Vec<_> = states
            .iter_mut()
            .map(|s| {
                let (mut v, mut pool) = s.take().unwrap();
                Builder::new()
                    .spawn(move || {
                        let mut r = Xorshift32(0x11451419);
                        for _ in 0..655360 {
                            let i = ((r.next() >> 8) & 511) as usize;
                            if v[i].is_some() {
                                pool.deallocate(v[i].take().unwrap());
                            } else {
                                v[i] = Some(pool.allocate(i));
                            }
                        }
                        let mut sum = 0;
                        for x in v.iter_mut() {
                            if let Some(x) = x.take() {
                                sum += pool[x];
                                pool.deallocate(x);
                            }
                        }
                        ((v, pool), sum)
                    })
                    .expect("failed to create thread")
            })
            .collect();
        let mut sum = 0;
        for (i, handle) in threads.drain(..).enumerate() {
            let (st, sub_sum) = handle.join().unwrap();
            states[i] = Some(st);
            sum += sub_sum;
        }
        sum
    });
}
