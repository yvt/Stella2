use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::convert::TryFrom;
use unicount::{num_scalars_in_str, str_next, str_prev};

struct Xorshift32(u32);

impl Iterator for Xorshift32 {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        Some(self.0)
    }
}

fn random_utf8(count: usize, rng: &mut Xorshift32) -> String {
    (0..count)
        .filter_map(|_| char::try_from(rng.next().unwrap() % 0x110000).ok())
        .collect()
}

fn test_str_next(s: &str, f: impl Fn(&str, usize) -> usize) -> usize {
    let mut i = 0;
    std::iter::from_fn(move || {
        if i >= s.len() {
            None
        } else {
            i = f(s, i);
            Some(())
        }
    })
    .count()
}

fn test_str_prev(s: &str, f: impl Fn(&str, usize) -> usize) -> usize {
    let mut i = s.len();
    std::iter::from_fn(move || {
        if i == 0 {
            None
        } else {
            i = f(s, i);
            Some(())
        }
    })
    .count()
}

fn criterion_benchmark(c: &mut Criterion) {
    for &len in &[4, 16, 65536] {
        {
            let mut group = c.benchmark_group("num_scalars_in_str");
            group.throughput(Throughput::Elements(len as u64));

            group.bench_function(BenchmarkId::new("unicount", len), move |b| {
                let st = random_utf8(len, &mut Xorshift32(42));
                let st = &st[..];
                b.iter(|| num_scalars_in_str(&st));
            });

            group.bench_function(BenchmarkId::new("str::chars", len), move |b| {
                let st = random_utf8(len, &mut Xorshift32(42));
                let st = &st[..];
                b.iter(|| st.chars().count());
            });
        }
        {
            let mut group = c.benchmark_group("str_next");
            group.throughput(Throughput::Elements(len as u64));

            group.bench_function(BenchmarkId::new("unicount", len), move |b| {
                let st = random_utf8(len, &mut Xorshift32(42));
                let st = &st[..];
                b.iter(|| test_str_next(&st, str_next));
            });

            group.bench_function(BenchmarkId::new("str::chars", len), move |b| {
                let st = random_utf8(len, &mut Xorshift32(42));
                let st = &st[..];
                b.iter(|| {
                    test_str_next(&st, |s, i| {
                        i + match s[i..].chars().nth(0) {
                            None => 0,
                            Some(c) => c.len_utf8(),
                        }
                    })
                });
            });
        }
        {
            let mut group = c.benchmark_group("str_prev");
            group.throughput(Throughput::Elements(len as u64));

            group.bench_function(BenchmarkId::new("unicount", len), move |b| {
                let st = random_utf8(len, &mut Xorshift32(42));
                let st = &st[..];
                b.iter(|| test_str_prev(&st, str_prev));
            });

            group.bench_function(BenchmarkId::new("str::chars", len), move |b| {
                let st = random_utf8(len, &mut Xorshift32(42));
                let st = &st[..];
                b.iter(|| {
                    test_str_prev(&st, |s, i| {
                        i - match s[..i].chars().rev().nth(0) {
                            None => 0,
                            Some(c) => c.len_utf8(),
                        }
                    })
                });
            });
        }
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
