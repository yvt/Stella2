//! A specialized, light-weight priority queue implementation for deferred
//! invocations (`Wm::invoke_after`).
//!
//! Invocations are referred to as *tasks*. Each task comprises of a start time,
//! a end time (deadline), and a payload of type `T`. The task must execute
//! between the start time and the deadline. Waking up a CPU incurs an energy
//! cost, so we optimize the power consumption by adjusting the timing to get as
//! many tasks as possible to execute at the same time.
//!
//! Tasks can be added or removed at any moment, which precludes the uses of
//! heap data structures.
//!
//! The number of tasks in the queue is expected to be very few — 8 at best.
//! Based on this condition, we set the following principles for the queue's
//! design:
//!
//!  - We employ linear-time search algorithms to minimize the code size
//!    overhead as well as to exploit the instruction-level parallelism
//!    offered by modern out-of-order processors by eliminating long dependency
//!    chains.
//!  - We set a hard limit on the queue length to allow placing the queue in
//!    a statically-allocated memory region, to make it easier for the
//!    compiler to eliminate bound checks, and to make it possible to embed
//!    indices in time values (see Time Values).
//!
//! # Time Values
//!
//! Time values are compared by reinterpreting them as `f64` because
//! `vpminuq`/`vpmaxuq` need AVX512F while `minpd`/`maxpd` are implemented by
//! all x86_64 processors. The value range is adjusted to avoid denormal numbers,
//! which severely slow down the computation and may be discarded if non-IEEE
//! compliant flags DAZ/FTZ are enabled. Negative numbers are also avoided
//! because their ordering is opposite between two's complement representation
//! and IEEE 754 binary64.
//!
//! # SIMD Width
//!
//! I chose `f64x4` (256-bit) to optimize for the mainstream processors used in
//! 2019 (in which AVX support is dominant) while balancing it against the
//! execution efficiency on legacy processors and a build configuration that do
//! not support 256-bit SIMD registers.
use alt_fp::FloatOrd;
use packed_simd::f64x4;
use std::{
    fmt,
    mem::MaybeUninit,
    ops::Range,
    time::{Duration, Instant},
};

// ============================================================================

pub struct TimerQueue<T> {
    core: TimerQueueCore<T>,
    origin: Instant,
}

impl<T> TimerQueue<T> {
    pub fn new() -> Self {
        Self {
            core: TimerQueueCore::new(),
            origin: Instant::now(),
        }
    }

    pub fn insert(&mut self, delay: Range<Duration>, payload: T) -> Result<HTask, CapacityError> {
        let offset = self.origin.elapsed();

        // Convert `Duration`s to `FixTime`s
        let time: Range<FixTime> = map_range(delay, |dur| (dur + offset).into());

        self.core.insert(time, payload)
    }

    pub fn remove(&mut self, htask: HTask) -> Option<T> {
        self.core.remove(htask)
    }

    pub fn runnable_tasks(&self) -> impl Iterator<Item = HTask> {
        self.core.runnable_tasks(self.origin.elapsed().into())
    }

    pub fn suggest_next_wakeup(&self) -> Option<Instant> {
        let time: Option<Duration> = self.core.suggest_next_wakeup().map(Into::into);

        time.map(|time| self.origin + time)
    }
}

impl<T: fmt::Debug> fmt::Debug for TimerQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        struct TaskMap<T>(T);

        impl<T: fmt::Debug> fmt::Debug for TaskMap<&'_ TimerQueueCore<T>> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_map()
                    .entries(self.0.iter().map(|(htask, time, payload)| {
                        (htask, (map_range(time, Into::<Duration>::into), payload))
                    }))
                    .finish()
            }
        }

        f.debug_struct("TimerQueue")
            .field("origin", &self.origin)
            .field("tasks", &TaskMap(&self.core))
            .finish()
    }
}

/// A time value in `0..TIME_MAX`.
#[derive(Debug, Clone, Copy, PartialOrd, PartialEq)]
struct FixTime(u64);

impl From<Duration> for FixTime {
    fn from(dur: Duration) -> FixTime {
        // It's extremely unlikely that our application runs for more than
        // 557,844 years without something breaking in the operating system, the
        // computer hardware, the power supply, and/or the human civilization or
        // whatever comes after (maybe Equestria), so we use `debug_assert!` here.
        // (But who knows, really?)
        debug_assert!(dur.as_secs() <= u64::max_value() >> 20);

        FixTime((dur.as_secs() << 20) + (dur.subsec_nanos() as u64 >> 10))
    }
}

impl Into<Duration> for FixTime {
    fn into(self) -> Duration {
        let secs = self.0 >> 20;
        let nanos = (self.0 & 0xfffff) << 10;
        Duration::new(secs, nanos as u32)
    }
}

// ============================================================================

const SIZE_BITS: u32 = 6;
const SIZE: usize = 1 << SIZE_BITS;

/// An unsigned integer type containing `SIZE` bits.
type Bitmap = u64;

const F64_MIN_NORMAL: u64 = 0x0010_0000_0000_0000;
const F64_MAX_NORMAL: u64 = 0x7fef_ffff_ffff_ffff;

const TIME_MAX: u64 = F64_MAX_NORMAL - F64_MIN_NORMAL;

/// The value of `TimerQueueCore::start` for vacant positions.
const VACANT_START: f64 = std::f64::INFINITY;
/// The value of `TimerQueueCore::end` for vacant positions.
const VACANT_END: f64 = std::f64::INFINITY;

struct TimerQueueCore<T> {
    /// The start time of each task.
    /// Must be `VACANT_START` for a vacant position.
    start: Array<[f64; SIZE]>,
    /// The end time (deadline) of each task.
    /// Must be `VACANT_START` for a vacant position.
    end: Array<[f64; SIZE]>,
    /// The payload of each task. Initialized iff the position is occpied.
    payloads: Array<[MaybeUninit<T>; SIZE]>,
    /// Each bit is set iff a task exists at the corresponding position.
    bitmap: Bitmap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityError;

impl FixTime {
    fn to_fp(self) -> f64 {
        // It's unrealistic for this assertion to fail, and the failure does
        // not cause a UB. Though `TimerQueueCore` will start acting weird.
        debug_assert!(self.0 <= TIME_MAX);

        f64::from_bits(F64_MIN_NORMAL + self.0)
    }

    fn from_fp(x: f64) -> Self {
        let bits = f64::to_bits(x);

        debug_assert!(bits >= F64_MIN_NORMAL);
        debug_assert!(bits <= F64_MAX_NORMAL);

        FixTime(bits - F64_MIN_NORMAL)
    }
}

impl<T> TimerQueueCore<T> {
    #[allow(clippy::uninit_assumed_init)] // allow `assume_init` for `Array<[MaybeUninit<_>; SIZE]>`
    fn new() -> Self {
        Self {
            start: Array([VACANT_START; SIZE]),
            end: Array([VACANT_END; SIZE]),
            // This is safe because what we are claiming to have initialized is
            // a bunch of `MaybeUninit`s, which do not require initialization.
            payloads: unsafe { MaybeUninit::uninit().assume_init() },
            bitmap: 0,
        }
    }

    fn insert(&mut self, time: Range<FixTime>, payload: T) -> Result<HTask, CapacityError> {
        let free_bitmap = !self.bitmap;

        if free_bitmap == 0 {
            return Err(CapacityError);
        }

        // Put the task on the vacant position
        let htask = trailing_zeros_as_htask(free_bitmap);

        debug_assert_eq!(self.start[htask].to_bits(), VACANT_START.to_bits());
        debug_assert_eq!(self.end[htask].to_bits(), VACANT_END.to_bits());
        self.start[htask] = time.start.to_fp();
        self.end[htask] = time.end.to_fp();

        debug_assert_eq!(self.bitmap & htask.mask(), 0);
        self.bitmap |= htask.mask();

        // This is actually not `unsafe`, because it's the same as calling
        // `MaybeUninit::write`, which is not `unsafe fn`.
        unsafe {
            self.payloads[htask].as_mut_ptr().write(payload);
        }

        Ok(htask)
    }

    fn remove(&mut self, htask: HTask) -> Option<T> {
        let mask = htask.mask();

        if (self.bitmap & mask) == 0 {
            // Vacant
            return None;
        }

        Some(unsafe { self.remove_unchecked(htask) })
    }

    unsafe fn remove_unchecked(&mut self, htask: HTask) -> T {
        let mask = htask.mask();

        // The position is occpied, remove the task
        debug_assert_ne!(self.start[htask].to_bits(), VACANT_START.to_bits());
        debug_assert_ne!(self.end[htask].to_bits(), VACANT_END.to_bits());
        self.start[htask] = VACANT_START;
        self.end[htask] = VACANT_END;

        self.bitmap &= !mask;

        // This is the `unsafe` part of this method
        self.payloads[htask].as_mut_ptr().read()
    }

    fn runnable_tasks(&self, time: FixTime) -> impl Iterator<Item = HTask> {
        let start = &self.start;
        let time = time.to_fp();

        let mut runnable_bitmap: Bitmap = 0;

        for htask in iter_bits_any_4x(self.bitmap) {
            // Should be lowered to a single 256-bit load
            let start = f64x4::new(
                start[htask.replace_bits(0b11, 0b00)],
                start[htask.replace_bits(0b11, 0b01)],
                start[htask.replace_bits(0b11, 0b10)],
                start[htask.replace_bits(0b11, 0b11)],
            );

            // `[i]` = `start[htask + i]` ≤ `time` for `i` ∈ `0..4`
            //         `false` if task `i` does not exist
            //         (`VACANT_START` needs to be NaN or +∞ for `<=` to
            //          evaluate to `false` in this case)
            let runnable = start.le(f64x4::splat(time)).bitmask();

            runnable_bitmap |= (runnable as Bitmap) << htask.replace_bits(0b11, 0b00).get();
        }

        // Must not return a non-existing task
        debug_assert_eq!(runnable_bitmap & !self.bitmap, 0);

        iter_bits(runnable_bitmap)
    }

    fn suggest_next_wakeup(&self) -> Option<FixTime> {
        let bitmap = self.bitmap;
        let end = &self.end;

        if bitmap == 0 {
            return None;
        }

        // Find the closest deadline, which is the upper bound of the solution.
        let min_end = iter_bits_any_4x(bitmap)
            .map(|htask| {
                // Should be lowered to a single 256-bit load
                f64x4::new(
                    end[htask.replace_bits(0b11, 0b00)],
                    end[htask.replace_bits(0b11, 0b01)],
                    end[htask.replace_bits(0b11, 0b10)],
                    end[htask.replace_bits(0b11, 0b11)],
                )
            })
            // Vacant elements are ignored here because `VACANT_END` is set to +∞.
            .fold(f64x4::splat(std::f64::INFINITY), FloatOrd::fmin);
        // TODO: Don't use `min_element`. It handles NaN correctly (thus it
        //       lowers to a native `minpd` + some extra instruction), but
        //       NaN never appears here.
        let min_end = min_end.min_element();

        debug_assert!(min_end.is_finite(), "{:?}", min_end);

        Some(FixTime::from_fp(min_end))
    }

    fn iter(&self) -> impl Iterator<Item = (HTask, Range<FixTime>, &T)> + '_ {
        iter_bits(self.bitmap).map(move |htask| {
            (
                htask,
                map_range(self.start[htask]..self.end[htask], FixTime::from_fp),
                // This is safe because `self.bitmap` says the position is occupied.
                unsafe { &*self.payloads[htask].as_ptr() },
            )
        })
    }
}

impl<T> Drop for TimerQueueCore<T> {
    fn drop(&mut self) {
        if !std::mem::needs_drop::<T>() {
            return;
        }

        for i in iter_bits(self.bitmap) {
            // This is safe because `self.bitmap` says the position is occupied.
            unsafe {
                self.payloads[i].as_mut_ptr().drop_in_place();
            }
        }
    }
}

// ============================================================================
// Utilities

fn map_range<T, S>(x: Range<T>, mut f: impl FnMut(T) -> S) -> Range<S> {
    f(x.start)..f(x.end)
}

/// Calculate `x.trailing_zeros()` and wrap it in `HTask` (that statically
/// guarantees the value fits in `0..SIZE`).
fn trailing_zeros_as_htask(x: Bitmap) -> HTask {
    assert!(std::mem::size_of::<Bitmap>() * 8 == SIZE);

    // This is safe because of the assertion above
    unsafe { HTask::new_unchecked(x.trailing_zeros() as usize) }
}

fn iter_bits(mut x: Bitmap) -> impl Iterator<Item = HTask> {
    std::iter::from_fn(move || {
        if x == 0 {
            None
        } else {
            let i = trailing_zeros_as_htask(x);
            x = (x - 1) & x; // clear the bit `i` - blsr (BMI1)
            Some(i)
        }
    })
}

/// Like `iter_bits`, but returns an element for each group of four bits
/// any of which are set.
///
/// Warning: Returned `HTask` must be aligned by `x.replace_bits(0b11, 0b00)`
/// before using it.
fn iter_bits_any_4x(mut x: Bitmap) -> impl Iterator<Item = HTask> {
    std::iter::from_fn(move || {
        if x == 0 {
            None
        } else {
            let i = trailing_zeros_as_htask(x);
            x &= !(0b1111u64 << i.replace_bits(0b11, 0b00).get() as u32);
            Some(i)
        }
    })
}

/// Constructing an unchecked `HTask` is `unsafe`, so hide the constructor by
/// wrapping it in a module. The unsafety of `get_unchecked[_mut]` is completely
/// isolated in this module.
mod utils {
    use super::{Bitmap, SIZE};
    use derive_more::{Deref, DerefMut};
    use std::ops::{Index, IndexMut};

    /// Represents a task in `TimerQueue`.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct HTask(u8);

    impl HTask {
        #[inline]
        #[allow(dead_code)]
        pub(super) fn new(x: usize) -> Self {
            assert!(x < super::SIZE);
            Self(x as u8)
        }

        pub(super) unsafe fn new_unchecked(x: usize) -> Self {
            Self(x as u8)
        }

        pub(super) fn get(self) -> usize {
            self.0 as usize
        }

        /// Get `1 << self.get()`.
        pub(super) fn mask(self) -> Bitmap {
            1u64 << self.0
        }

        /// Replace the bits specified by `mask` with `value`.
        #[inline]
        pub(super) fn replace_bits(self, mask: u8, value: u8) -> Self {
            use std::convert::TryInto;
            assert!(mask < super::SIZE.try_into().unwrap());

            Self((self.0 & !mask) + (value & mask))
        }
    }

    /// Wraps `[T; SIZE]` to safely skip bound checks when indexed by `HTask`.
    #[derive(Debug, Clone, Copy, Deref, DerefMut)]
    #[repr(transparent)]
    pub struct Array<T>(pub T);

    impl<T> Index<HTask> for Array<[T; SIZE]> {
        type Output = T;
        fn index(&self, index: HTask) -> &Self::Output {
            unsafe { self.0.get_unchecked(index.0 as usize) }
        }
    }

    impl<T> IndexMut<HTask> for Array<[T; SIZE]> {
        fn index_mut(&mut self, index: HTask) -> &mut Self::Output {
            unsafe { self.0.get_unchecked_mut(index.0 as usize) }
        }
    }
}

use self::utils::Array;
pub use self::utils::HTask;

#[cfg(test)]
mod tests {
    use cgmath::assert_abs_diff_eq;
    use log::{debug, error};
    use quickcheck_macros::quickcheck;

    use super::*;

    #[test]
    fn fix_time_cvt() {
        for &d in &[
            Duration::from_nanos(0),
            Duration::from_nanos(300),
            Duration::from_micros(1),
            Duration::from_micros(10),
            Duration::from_millis(1),
            Duration::from_secs(1),
            Duration::from_secs(5),                     // The World
            Duration::from_secs(60),                    // 1 minute
            Duration::from_secs(86400),                 // 1 Earth day
            Duration::from_secs(86400 * 12),            // Akechi Mitsuhide's reign
            Duration::from_secs_f64(86400.0 * 87.9691), // 1 Mercurian year
            Duration::from_secs(86400 * 365 * 4),       // 1 olympiad
            Duration::from_secs(86400 * 365 * 1000),    // Nightmare Moon's imprisonment
            // (assuming 1 Equestrian year = 1 Earth year)
            Duration::from_secs(86400 * 365 * 1849), // The Roman empire
        ] {
            let ft: FixTime = d.into();
            let d2: Duration = ft.into();

            // Should have a microsecond precision
            assert_abs_diff_eq!(d2.as_secs_f64(), d.as_secs_f64(), epsilon = 2.0e-6);
        }
    }

    #[quickcheck]
    fn iter_bits_test(mut bits: Vec<usize>) -> bool {
        for bit in bits.iter_mut() {
            *bit = *bit & 63;
        }
        bits.sort();
        bits.dedup();

        let bitmap = bits.iter().fold(0u64, |x, i| x | (1u64 << i));
        debug!("bitmap = 0x{:08x}", bitmap);

        let out_bits = iter_bits(bitmap).map(HTask::get).collect::<Vec<_>>();
        debug!("got {:?}, expected {:?}", out_bits, bits);

        out_bits == bits
    }

    #[quickcheck]
    fn iter_bits_any_4x_test(mut bits: Vec<usize>) -> bool {
        for bit in bits.iter_mut() {
            *bit = *bit & 63;
        }
        bits.sort();

        // Simulate `iter_bits_any_4x`
        let mut bits_any_4x = Vec::new();
        let mut min = 0;
        for &bit in bits.iter() {
            if bit >= min {
                bits_any_4x.push(bit);

                // Next four-bit group
                min = bit / 4 * 4 + 4;
            }
        }

        let bitmap = bits.iter().fold(0u64, |x, i| x | (1u64 << i));
        debug!("bitmap = 0x{:08x}", bitmap);

        let out_bits = iter_bits_any_4x(bitmap).map(HTask::get).collect::<Vec<_>>();
        debug!("got {:?}, expected {:?}", out_bits, bits_any_4x);

        out_bits == bits_any_4x
    }

    #[test]
    fn capacity_error() {
        let mut queue = TimerQueueCore::new();
        for _ in 0..SIZE {
            queue.insert(FixTime(0)..FixTime(100), ()).unwrap();
        }
        assert_eq!(
            queue.insert(FixTime(0)..FixTime(100), ()),
            Err(CapacityError)
        );
    }

    #[quickcheck]
    fn schedules_correctly(data: Vec<u64>, use_tolerance: bool) -> bool {
        struct Task {
            time: Range<FixTime>,
            did_run: bool,
        }

        let mut tasks: Vec<_> = data
            .chunks_exact(2)
            .map(|chunk| Task {
                time: if use_tolerance {
                    FixTime(chunk[0])..FixTime(chunk[0] + chunk[1])
                } else {
                    FixTime(chunk[0])..FixTime(chunk[0])
                },
                did_run: false,
            })
            .take(SIZE)
            .collect();

        let mut queue = TimerQueueCore::new();
        for (i, task) in tasks.iter().enumerate() {
            let htask = queue.insert(task.time.clone(), i);
            debug!("Enqueued {:?} as {:?}", (i, &task.time), htask);
        }

        let mut limit = SIZE;
        while let Some(t) = queue.suggest_next_wakeup() {
            debug!("Waking up at {:?}", t);

            for htask in queue.runnable_tasks(t) {
                let i = queue.remove(htask).unwrap();
                debug!("  Completed task {:?}", i);

                let time_constraint = &tasks[i].time;
                if t < time_constraint.start || t > time_constraint.end {
                    error!(
                        "  Constraint violation: {:?} ∉ {:?}..={:?}",
                        t, time_constraint.start, time_constraint.end
                    );
                }

                tasks[i].did_run = true;
            }

            limit -= 1;
            if limit == 0 {
                error!("Did not complete within a time limit");
                return false;
            }
        }

        for (i, task) in tasks.iter().enumerate() {
            if !task.did_run {
                error!("Task {:?} did not run", i);
                return false;
            }
        }

        true
    }
}
