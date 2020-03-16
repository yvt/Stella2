use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::convert::TryFrom;
use utf16count::utf16_len;

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

fn criterion_benchmark(c: &mut Criterion) {
    for &len in &[4, 16, 65536] {
        let mut group = c.benchmark_group("sort");

        group.throughput(Throughput::Elements(len as u64));

        group.bench_function(BenchmarkId::new("utf16count", len), move |b| {
            let st = random_utf8(len, &mut Xorshift32(42));
            let st = &st[..];
            b.iter(|| utf16_len(&st));
        });

        group.bench_function(BenchmarkId::new("str::encode_utf16", len), move |b| {
            let st = random_utf8(len, &mut Xorshift32(42));
            let st = &st[..];
            b.iter(|| st.encode_utf16().count());
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
