use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use minisort::insertion_sort;

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

fn fill<T>(a: &mut [T], i: impl IntoIterator<Item = T>) {
    for (a, value) in a.iter_mut().zip(i) {
        *a = value;
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let sizes: Vec<_> = (0..10).map(|i| 1usize << i).collect();

    let mut group = c.benchmark_group("sort");
    for &size in &sizes {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_function(BenchmarkId::new("none", size), move |b| {
            let mut array = vec![0u32; size];

            b.iter(|| {
                let array = black_box(&mut array[..]);
                fill(array, Xorshift32(42));
            });
        });
        group.bench_function(BenchmarkId::new("insertion_sort", size), move |b| {
            let mut array = vec![0u32; size];

            b.iter(|| {
                let array = black_box(&mut array[..]);
                fill(array, Xorshift32(42));
                insertion_sort(array);
            });
        });

        group.bench_function(BenchmarkId::new("std", size), move |b| {
            let mut array = vec![0u32; size];

            b.iter(|| {
                let array = black_box(&mut array[..]);
                fill(array, Xorshift32(42));
                array.sort();
            });
        });

        group.bench_function(BenchmarkId::new("std_unstable", size), move |b| {
            let mut array = vec![0u32; size];

            b.iter(|| {
                let array = black_box(&mut array[..]);
                fill(array, Xorshift32(42));
                array.sort_unstable();
            });
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
