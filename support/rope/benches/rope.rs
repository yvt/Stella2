use bencher::{benchmark_group, benchmark_main, Bencher};

use rope::{by_ord, One, Rope};

struct Xorshift32(u32);

impl Xorshift32 {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

fn bench_iter(b: &mut Bencher, count: usize) {
    let rope: Rope<_> = (0..count).map(|x| x.to_string()).collect();

    let mut it = rope.iter();

    b.iter(|| {
        loop {
            if let Some(e) = it.next() {
                break e;
            } else {
                it = rope.iter();
            }
        }
    });
}

fn bench_iter_100000(b: &mut Bencher) {
    bench_iter(b, 100000);
}
fn bench_iter_010000(b: &mut Bencher) {
    bench_iter(b, 10000);
}
fn bench_iter_001000(b: &mut Bencher) {
    bench_iter(b, 1000);
}
fn bench_iter_000100(b: &mut Bencher) {
    bench_iter(b, 100);
}
fn bench_iter_000010(b: &mut Bencher) {
    bench_iter(b, 10);
}

benchmark_group!(
    group_bench_iter,
    bench_iter_100000,
    bench_iter_010000,
    bench_iter_001000,
    bench_iter_000100,
    bench_iter_000010,
);

fn bench_search_seq(b: &mut Bencher, count: usize) {
    let rope: Rope<_> = (0..count).map(|x| x.to_string()).collect();
    let len = rope.offset_len();

    let mut i = 0;

    b.iter(|| {
        i = (i + 1) % len;
        rope.get(One::FirstAfter(by_ord(i as isize)))
    });
}

fn bench_search_seq_100000(b: &mut Bencher) {
    bench_search_seq(b, 100000);
}
fn bench_search_seq_010000(b: &mut Bencher) {
    bench_search_seq(b, 10000);
}
fn bench_search_seq_001000(b: &mut Bencher) {
    bench_search_seq(b, 1000);
}
fn bench_search_seq_000100(b: &mut Bencher) {
    bench_search_seq(b, 100);
}
fn bench_search_seq_000010(b: &mut Bencher) {
    bench_search_seq(b, 10);
}

benchmark_group!(
    group_bench_search_seq,
    bench_search_seq_100000,
    bench_search_seq_010000,
    bench_search_seq_001000,
    bench_search_seq_000100,
    bench_search_seq_000010,
);

fn bench_search_random(b: &mut Bencher, count: usize) {
    let rope: Rope<_> = (0..count).map(|x| x.to_string()).collect();
    let len = rope.offset_len();

    let mut rng = Xorshift32(10000);

    b.iter(|| {
        let i = rng.next() % (len + 1) as u32;
        rope.get(One::FirstAfter(by_ord(i as isize)))
    });
}

fn bench_search_random_100000(b: &mut Bencher) {
    bench_search_random(b, 100000);
}
fn bench_search_random_010000(b: &mut Bencher) {
    bench_search_random(b, 10000);
}
fn bench_search_random_001000(b: &mut Bencher) {
    bench_search_random(b, 1000);
}
fn bench_search_random_000100(b: &mut Bencher) {
    bench_search_random(b, 100);
}
fn bench_search_random_000010(b: &mut Bencher) {
    bench_search_random(b, 10);
}

benchmark_group!(
    group_bench_search_random,
    bench_search_random_100000,
    bench_search_random_010000,
    bench_search_random_001000,
    bench_search_random_000100,
    bench_search_random_000010,
);

benchmark_main!(
    group_bench_iter,
    group_bench_search_seq,
    group_bench_search_random
);
