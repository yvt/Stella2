use bencher::{benchmark_group, benchmark_main, Bencher};

use alt_fp::{f32_to_u23, u23_to_f32};

fn run_u32_to_f32(b: &mut Bencher, cvt: impl Fn(u32) -> f32) {
    // Make sure the array is smaller than L1D$ so that the loop does not
    // get satured by the memory system
    let t1: Vec<u32> = (0..2048).map(|x| x & 31).collect();
    let mut t2: Vec<f32> = vec![0.0; t1.len()];

    b.iter(|| {
        for _ in 0..1000 {
            for (x, y) in t1.iter().zip(t2.iter_mut()) {
                *y = cvt(*x);
            }
        }
    });
}

fn bench_u32_to_f32_sys(b: &mut Bencher) {
    run_u32_to_f32(b, |x| x as _);
}

fn bench_u32_to_f32(b: &mut Bencher) {
    run_u32_to_f32(b, u23_to_f32);
}

fn run_f32_to_u32(b: &mut Bencher, cvt: impl Fn(f32) -> u32) {
    // Make sure the array is smaller than L1D$ so that the loop does not
    // get satured by the memory system
    let t1: Vec<f32> = (0..2048).map(|x| (x & 31) as f32).collect();
    let mut t2: Vec<u32> = vec![0; t1.len()];

    b.iter(|| {
        for _ in 0..1000 {
            for (x, y) in t1.iter().zip(t2.iter_mut()) {
                *y = cvt(*x);
            }
        }
    });
}

fn bench_f32_to_u32_sys(b: &mut Bencher) {
    run_f32_to_u32(b, |x| x as _);
}

fn bench_f32_to_u32(b: &mut Bencher) {
    run_f32_to_u32(b, f32_to_u23);
}

benchmark_group!(
    benches,
    bench_u32_to_f32_sys,
    bench_u32_to_f32,
    bench_f32_to_u32_sys,
    bench_f32_to_u32,
);
benchmark_main!(benches);
