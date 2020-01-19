use std::{
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nativedispatch::Queue;

fn criterion_benchmark(c: &mut Criterion) {
    let sizes: Vec<_> = (0..12).map(|i| 1usize << i).collect();

    let mut group = c.benchmark_group("invoke");
    for &size in &sizes {
        group.throughput(Throughput::Elements(size as u64));

        group.bench_function(BenchmarkId::new("parallel", size), move |b| {
            b.iter(|| {
                static COUNT: AtomicUsize = AtomicUsize::new(0);

                let cur_thread = thread::current();
                let queue = Queue::global_med();
                COUNT.store(size, Ordering::Relaxed);

                // Spawn independent tasks
                for _ in 0..size {
                    // Extend the lifetime of `cur_thread`
                    let cur_thread = unsafe { &*((&cur_thread) as *const thread::Thread) };
                    queue.invoke(move || {
                        if COUNT.fetch_sub(1, Ordering::Relaxed) == 1 {
                            cur_thread.unpark();
                        }
                    });
                }

                // Wait until all tasks are complete
                while COUNT.load(Ordering::Relaxed) > 0 {
                    thread::park();
                }
            });
        });

        group.bench_function(BenchmarkId::new("serial", size), move |b| {
            b.iter(|| {
                static DONE: AtomicUsize = AtomicUsize::new(0);
                DONE.store(0, Ordering::Relaxed);

                struct Ctx {
                    parent_thread: thread::Thread,
                    queue: Queue,
                }

                let ctx = Ctx {
                    parent_thread: thread::current(),
                    queue: Queue::global_med(),
                };

                // Spawn a task, which will spawn the next task in quick succession
                fn task_body(ctx: &'static Ctx, count: usize) {
                    if count == 0 {
                        DONE.store(1, Ordering::Relaxed);
                        ctx.parent_thread.unpark();
                    } else {
                        ctx.queue.invoke(move || {
                            task_body(ctx, count - 1);
                        });
                    }
                }

                // Extend the lifetime of `ctx`
                task_body(unsafe { &*((&ctx) as *const Ctx) }, size);

                // Wait until all tasks are complete
                while DONE.load(Ordering::Relaxed) == 0 {
                    thread::park();
                }
            });
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
