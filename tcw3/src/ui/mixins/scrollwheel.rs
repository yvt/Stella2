// FIXME: `scrollwheel` might not be the best naming because we don't
//        actually have an event named "scroll wheel". In an earlier version,
//        this module was named `scrollable`, but that gives a false implication
//        that it handles everything related to scrolling, like scrollbars.
use alt_fp::FloatOrd;
use cggeom::{prelude::*, Box2};
use cgmath::{Point2, Vector2};
use log::trace;
use std::{cell::Cell, rc::Rc};

use crate::{
    pal,
    prelude::*,
    uicore::{HViewRef, HWndRef, ScrollDelta, ScrollListener},
};

/// A view listener mix-in that facilitates scrolling and provides a consistent
/// experience.
#[derive(Debug)]
pub struct ScrollWheelMixin {
    inner: Rc<Inner>,
}

bitflags::bitflags! {
    /// Specifies the axes [`ScrollWheelMixin`] can scroll along.
    pub struct ScrollAxisFlags: u32 {
        const HORIZONTAL = 1;
        const VERTICAL = 1 << 1;
        const BOTH = Self::HORIZONTAL.bits | Self::VERTICAL.bits;
    }
}

/// Provides methods to query and manipulate the scrolling state of
/// scrollable contents.
///
/// This object is re-created every time the scrolling state is about to change.
/// Each created object can have independent points of reference to allow,
/// for example, the scrollable bounds to change dynamically. This design is
/// mainly to support `tcw3::ui::views::table::Table`, which dynamically resizes
/// rows and columns frequently and invalidates any saved position values.
pub trait ScrollModel {
    /// Get the lower and upper bounds.
    fn bounds(&mut self) -> Box2<f64>;

    /// Get the current scroll position.
    fn pos(&mut self) -> Point2<f64>;

    /// Set a new scroll position. It is usually contained by or on the
    /// boundary of `self.bounds()`, but might not be during animation.
    fn set_pos(&mut self, value: Point2<f64>);

    /// Get the size of columns and rows.
    fn line_size(&mut self) -> [f64; 2] {
        [15.0; 2]
    }

    /// Revert the scroll position to the original position (i.e., the position
    /// when `handle_scroll_gesture` is called). This method is only used with
    /// `handle_scroll_gesture`.
    fn cancel(&mut self);
}

/// A no-op implementation of `ScrollModel`, which is useful when a factory
/// function is called but the target object is no longer accessible.
impl ScrollModel for () {
    fn bounds(&mut self) -> Box2<f64> {
        Box2::zero()
    }
    fn pos(&mut self) -> Point2<f64> {
        [0.0; 2].into()
    }
    fn set_pos(&mut self, _value: Point2<f64>) {}
    fn cancel(&mut self) {}
}

impl<T: ScrollModel + 'static> From<T> for Box<dyn ScrollModel> {
    fn from(x: T) -> Box<dyn ScrollModel> {
        Box::new(x)
    }
}

impl Default for ScrollWheelMixin {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollWheelMixin {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                token: Cell::new(0),
                delta: Cell::new(ScrollDelta::default()),
                flush_enqueued: Cell::new(false),
                axes: Cell::new(ScrollAxisFlags::BOTH),
            }),
        }
    }

    /// Set the axes for which scrolling is allowed.
    ///
    /// This might not take effect for an ongoing scroll gesture (if any).
    pub fn set_axes(&self, axes: ScrollAxisFlags) {
        self.inner.axes.set(axes);
    }

    /// Stop the current scroll action and animation.
    pub fn stop(&self) {
        self.inner.stop();
    }

    /// Handles [`ViewListener::scroll_motion`].
    ///
    /// [`ViewListener::scroll_motion`]: crate::uicore::ViewListener::scroll_motion
    pub fn scroll_motion(
        &self,
        wm: pal::Wm,
        delta: &ScrollDelta,
        model_getter: impl FnOnce() -> Box<dyn ScrollModel> + 'static,
    ) {
        self.inner.stop_except_stateless_scroll();

        let delta = ScrollDelta {
            delta: filter_vec_by_axis_flags(delta.delta, self.inner.axes.get()),
            ..*delta
        };

        if delta.delta == Vector2::new(0.0, 0.0) {
            return;
        }

        self.inner.accumulate(&delta);

        if (self.inner.token.get() & STATELESS_SCROLL_TOKEN_FLAG) != 0 {
            // There already is a pending `invoke_on_update`
            return;
        }

        let inner = Rc::clone(&self.inner);

        // Mark that this token is for `scroll_motion`. The marked token is
        // not affected by `stop_except_stateless_scroll`, so multiple calls to
        // `scroll_motion` are accumulated until the closure passed to
        // `invoke_on_update` is finally called.
        inner
            .token
            .set(inner.token.get() | STATELESS_SCROLL_TOKEN_FLAG);
        let token = inner.token.get();

        wm.invoke_on_update(move |_| {
            inner.stateless_scroll_inner(model_getter(), token);
        });
    }

    /// Handles [`ViewListener::scroll_gesture`].
    ///
    /// [`ViewListener::scroll_gesture`]: crate::uicore::ViewListener::scroll_gesture
    pub fn scroll_gesture(
        &self,
        model_getter: impl Fn() -> Box<dyn ScrollModel> + 'static,
    ) -> Box<dyn ScrollListener> {
        self.inner_scroll_gesture(Rc::new(model_getter))
    }

    // Non-generic inner function
    fn inner_scroll_gesture(
        &self,
        model_getter: Rc<dyn Fn() -> Box<dyn ScrollModel>>,
    ) -> Box<dyn ScrollListener> {
        self.inner.stop();

        self.inner.flush_enqueued.set(false);

        Box::new(ScrollListenerImpl {
            inner: Rc::clone(&self.inner),
            model_getter,
            momentum: Cell::new(false),
            vertical: Cell::new(false),
            token: self.inner.token.get(),
            velocity: Cell::new((0.0, true)),
        })
    }
}

#[derive(Debug)]
struct Inner {
    /// An identifier for the currently active drag gesture.
    token: Cell<u64>,
    /// Accumulates the scroll amount.
    delta: Cell<ScrollDelta>,
    /// `ScrollListenerImpl::motion` called `invoke_on_update`.
    flush_enqueued: Cell<bool>,
    /// The axes for which scrolling is allowed.
    axes: Cell<ScrollAxisFlags>,
}

/// This flag indicates `Inner::token` corresponds to a scroll action registered
/// through `ScrollWheelMixin::scroll_motion`.
const STATELESS_SCROLL_TOKEN_FLAG: u64 = 1;

const BOUNCE_TIME: f32 = 0.4;
const RELAXATION_TIME: f32 = 0.7;

const BOUNCE_OVERSHOOT_LIMIT: f32 = 50.0;

impl Inner {
    /// Stop the current scroll action and animation and issue a new `token`.
    fn stop(&self) {
        // Scroll actions have a copy of this value, and stop if they see a
        // different token value
        self.token.set(
            (self.token.get() & !STATELESS_SCROLL_TOKEN_FLAG)
                .checked_add(2)
                .expect("Scroll action token exhausted"),
        );

        self.delta.set(ScrollDelta::default());
    }

    fn stop_except_stateless_scroll(&self) {
        if (self.token.get() & STATELESS_SCROLL_TOKEN_FLAG) == 0 {
            self.stop();
        }
    }

    fn stateless_scroll_inner(&self, mut model: Box<dyn ScrollModel>, token: u64) {
        if self.token.get() != token {
            // Cancelled
            return;
        }

        // Clear the flag
        self.token.set(token & !STATELESS_SCROLL_TOKEN_FLAG);

        self.clamping_flush(&mut *model);
    }

    fn accumulate(&self, delta: &ScrollDelta) {
        let mut accum = self.delta.get();
        if accum.precise == delta.precise {
            accum.delta += delta.delta;
        } else {
            accum = *delta;
        }
        self.delta.set(accum);
    }

    fn take_delta(&self, model: &mut dyn ScrollModel) -> Vector2<f64> {
        let delta = self.delta.take();
        let mut dt = delta.delta.cast::<f64>().unwrap();

        if !delta.precise {
            let line_size = model.line_size();
            dt.x *= line_size[0];
            dt.y *= line_size[1];
        }

        dt
    }

    fn clamping_flush(&self, model: &mut dyn ScrollModel) {
        let dt = self.take_delta(model);
        let pos = model.bounds().limit_point(&(model.pos() - dt));
        model.set_pos(pos);
    }

    fn overflowing_flush(&self, model: &mut dyn ScrollModel, vertical: bool) {
        let dt = self.take_delta(model);

        let mut raw_pos = model.pos();
        let bounds = model.bounds();

        let i = vertical as usize;

        let pos = inverse_smooth_clamp_twoside(raw_pos[i], bounds.min[i]..bounds.max[i]);
        let pos = pos - dt[i];
        let pos = smooth_clamp_twoside(pos, bounds.min[i]..bounds.max[i]);

        raw_pos[i] = pos;

        // Bound the other axis' scroll position so that momentum scrolling is
        // not prevented
        const TOLERANCE: f64 = 2.0;
        let k = 1 - i;
        let sec_pos = raw_pos[k].fmax(bounds.min[k]).fmin(bounds.max[k]);
        if (sec_pos - raw_pos[k]).abs() < TOLERANCE {
            raw_pos[k] = sec_pos;
        }

        model.set_pos(raw_pos);
    }

    /// Handles movement in the `Momentum` state. May transition into `Bounce`.
    fn bouncing_flush(
        this: Rc<Self>,
        hview: HViewRef<'_>,
        vertical: bool,
        (velocity, velocity_precise): (f32, bool),
        model_getter: Rc<dyn Fn() -> Box<dyn ScrollModel>>,
    ) {
        let mut model = model_getter();

        // In this state, movement is limited to a single axis.
        let axis = vertical as usize;
        let dt = this.take_delta(&mut *model)[axis];

        let mut pos = model.pos();
        let bounds = model.bounds();
        let line_size = model.line_size();

        debug_assert!(bounds.contains_point_incl(&pos));
        pos[axis] -= dt;

        let overflow_upper = pos[axis] > bounds.max[axis];
        let overflow_lower = pos[axis] < bounds.min[axis];

        pos[axis] = pos[axis].fmin(bounds.max[axis]).fmax(bounds.min[axis]);
        model.set_pos(pos);

        drop(model);

        if overflow_upper || overflow_lower {
            debug_assert!(!overflow_upper || !overflow_lower);

            let velocity = if velocity_precise {
                velocity
            } else {
                velocity * line_size[axis] as f32
            };

            trace!(
                "pos ({:?}) hit the bounds ({:?}), start bouncing \
                 (vertical = {:?}, velocity = {:?})",
                pos[axis],
                bounds.min[axis]..bounds.max[axis],
                vertical,
                velocity
            );

            // Start bouncing
            Self::start_bounce(this, hview, vertical, velocity, model_getter);
        }
    }

    fn start_bounce(
        this: Rc<Self>,
        hview: HViewRef<'_>,
        vertical: bool,
        velocity: f32,
        model_getter: Rc<dyn Fn() -> Box<dyn ScrollModel>>,
    ) {
        this.stop();

        let hwnd = if let Some(hwnd) = hview.containing_wnd() {
            hwnd
        } else {
            return;
        };

        let token = this.token.get();

        let position = Cell::new(0.0);

        start_transition(hwnd.as_ref(), BOUNCE_TIME, move |_, progress| {
            if token != this.token.get() {
                return false;
            }

            let mut model = model_getter();

            if progress >= 1.0 {
                // The animation is complete
                clamp_model_pos(&mut *model);
                return false;
            }

            // Locate the first Bézier control point in such a way that the
            // velocity is preserved at the start of the animation. Limit the
            // length of the control handle (an imaginary straight line between
            // the starting point and the control point) so that the movement
            // doesn't go too far.
            let (mut x1, mut y1) = (0.15, velocity * BOUNCE_TIME * 0.15);
            let max_y1 = BOUNCE_OVERSHOOT_LIMIT * 2.0;
            if y1.abs() > max_y1 {
                x1 *= max_y1 / y1.abs();
                y1 = max_y1.copysign(y1);
            }

            // Evaluate the animation
            let overflow = eval_bezier_bouncing_timing_func(x1, y1, 0.15, 0.0, progress);
            let delta = overflow - position.get();
            position.set(overflow);

            let mut pos = model.pos();
            let axis = vertical as usize;
            pos[axis] -= delta as f64;
            model.set_pos(pos);

            trace!(
                "overflow = {:?}, delta = {:?}, pos = {:?}",
                overflow,
                delta,
                pos[axis]
            );

            true
        });
    }

    fn start_relaxation(
        this: Rc<Self>,
        hview: HViewRef<'_>,
        model_getter: Rc<dyn Fn() -> Box<dyn ScrollModel>>,
    ) {
        this.stop();

        let hwnd = if let Some(hwnd) = hview.containing_wnd() {
            hwnd
        } else {
            return;
        };

        let token = this.token.get();

        let position = Cell::new(0.0);

        let mut model = model_getter();
        let pos = model.pos();
        let goal = model.bounds().limit_point(&pos) - pos;
        drop(model);

        start_transition(hwnd.as_ref(), RELAXATION_TIME, move |_, progress| {
            if token != this.token.get() {
                return false;
            }

            let mut model = model_getter();

            if progress >= 1.0 {
                // The animation is complete
                clamp_model_pos(&mut *model);
                return false;
            }

            // Evaluate the animation. This is an extreme version of `ease-out`.
            let xformed = eval_bezier_bouncing_timing_func(0.13, 1.0, 0.25, 1.0, progress);
            let delta = xformed - position.get();
            position.set(xformed);

            let mut pos = model.pos();
            pos += goal * delta as f64;
            model.set_pos(pos);

            trace!(
                "xformed = {:?}, delta = {:?}, pos = {:?}",
                xformed,
                delta,
                pos
            );

            true
        });
    }
}

#[rustfmt::skip]
fn filter_vec_by_axis_flags(x: Vector2<f32>, flags: ScrollAxisFlags) -> Vector2<f32> {
    [
        if flags.contains(ScrollAxisFlags::HORIZONTAL) { x.x } else { 0.0 },
        if flags.contains(ScrollAxisFlags::VERTICAL) { x.y } else { 0.0 },
    ].into()
}

fn clamp_model_pos(model: &mut dyn ScrollModel) {
    let pos = model.bounds().limit_point(&model.pos());
    model.set_pos(pos);
}

/// Implements `ScrollListener`.
///
/// # States
///
/// ```text
///
///             cancel
///  Initial -----------> Done <----------------------,
///    |                   ^ ^             ^          |
///    |  end in-bounds    | |             | settle   | settle
///    +-------------------' '---------,   |   go-OOB | OR cancel
///    |                        settle |  Bounce <----+
///    |  end out-of-bounds            |              |
///    +-------------------------> Relaxation         |
///    |                               ^              |
///    |  start-momentum out-of-bounds |              |
///    +-------------------------------'           Momentum
///    |                                              ^
///    |  start-momentum in-bounds                    |
///    '----------------------------------------------'
///
/// ```
///
/// `ScrollListenerImpl` handles `Initial` and `Momentum`. The rest of the
/// states are handled by `start_bounce` and `start_relaxation` that create
/// a timer loop that is independent from scroll events.
#[derive(Clone)]
#[repr(align(32))] // hopefully makes cloning fast
struct ScrollListenerImpl {
    inner: Rc<Inner>,
    model_getter: Rc<dyn Fn() -> Box<dyn ScrollModel>>,
    momentum: Cell<bool>,
    /// The scroll direction. The direction is contrained to one axis because
    /// we don't want to handle two separate instances of the bounce effect at
    /// the same time.
    vertical: Cell<bool>,
    token: u64,
    /// The last known velocity. The second field contains the value of
    /// `precise`.
    velocity: Cell<(f32, bool)>,
}

impl ScrollListenerImpl {
    /// Returns `false` if the action was cancelled.
    fn is_valid(&self) -> bool {
        self.token == self.inner.token.get()
    }
}

impl ScrollListener for ScrollListenerImpl {
    fn motion(
        &self,
        wm: pal::Wm,
        hview: HViewRef<'_>,
        delta: &ScrollDelta,
        velocity: Vector2<f32>,
    ) {
        let delta = ScrollDelta {
            delta: filter_vec_by_axis_flags(delta.delta, self.inner.axes.get()),
            ..*delta
        };

        if delta.delta == Vector2::new(0.0, 0.0) || !self.is_valid() {
            return;
        }

        let is_momentum_state = self.momentum.get();
        if !is_momentum_state {
            // Reset the direction based on the latest velocity
            self.vertical.set(velocity.y.abs() > velocity.x.abs());
        }
        let axis = self.vertical.get() as usize;
        self.velocity.set((velocity[axis], delta.precise));

        self.inner.accumulate(&delta);

        if !self.inner.flush_enqueued.get() {
            self.inner.flush_enqueued.set(true);

            let this = self.clone();
            let hview = hview.upgrade();

            wm.invoke_on_update(move |_| {
                let inner = &this.inner;
                if this.token != inner.token.get() {
                    return;
                }

                inner.flush_enqueued.set(false);

                if this.momentum.get() {
                    Inner::bouncing_flush(
                        this.inner,
                        hview.as_ref(),
                        this.vertical.get(),
                        this.velocity.get(),
                        this.model_getter,
                    );
                } else {
                    let mut model = (this.model_getter)();
                    inner.overflowing_flush(&mut *model, this.vertical.get());
                }
            });
        }
    }

    fn start_momentum_phase(&self, _: pal::Wm, hview: HViewRef<'_>) {
        let mut model = (self.model_getter)();
        let pos = model.pos();
        let bounds = model.bounds();

        if !bounds.contains_point_incl(&pos) {
            trace!(
                "{:?} is outside {:?}, start relaxation",
                pos,
                bounds.display_im()
            );

            // Start the relaxation motion to restore a valid scroll position.
            // No momentum scrolling in this case. `self` is invalidated by
            // `start_relaxation` by modifying `inner.token`.
            Inner::start_relaxation(Rc::clone(&self.inner), hview, Rc::clone(&self.model_getter));
        } else {
            trace!(
                "{:?} is inside {:?}, allowing momentum scrolling",
                pos,
                bounds.display_im()
            );
        }

        self.momentum.set(true);
    }

    fn end(&self, _: pal::Wm, hview: HViewRef<'_>) {
        if !self.is_valid() {
            return;
        }

        let mut model = (self.model_getter)();
        let pos = model.pos();
        let bounds = model.bounds();

        if bounds.contains_point_incl(&pos) {
            trace!(
                "{:?} is inside {:?}, stopping scrolling",
                pos,
                bounds.display_im()
            );
            return;
        }

        trace!(
            "{:?} is outside {:?}, start relaxation",
            pos,
            bounds.display_im()
        );

        // Start the relaxation motion to restore a valid scroll position
        // `self` is invalidated by `start_relaxation` by modifying `inner.token`.
        Inner::start_relaxation(Rc::clone(&self.inner), hview, Rc::clone(&self.model_getter));
    }

    fn cancel(&self, _: pal::Wm, _: HViewRef<'_>) {
        if !self.is_valid() {
            return;
        }

        (self.model_getter)().cancel();
    }
}

const OVERFLOW_COMPRESS: f64 = 10.0;

const OVERFLOW_LIMIT: f64 = 40.0;

/// This value is used to clamp the result of `smooth_clamp` so that it never
/// reaches `OVERFLOW_LIMIT`. The clamping is required because the inverse
/// function is undefined at `OVERFLOW_LIMIT`. Therefore this value must be
/// slightly less than `OVERFLOW_LIMIT`.
const OVERFLOW_CLAMP: f64 = OVERFLOW_LIMIT - 5.0;

/// Like `min(x, OVERFLOW_LIMIT)`, but smooth and asymptotically approaches
/// `OVERFLOW_LIMIT`. This function satisfies the following properties for all
/// `x ≥ 0`: `f(0) = 0`, `f'(0) = 1 / OVERFLOW_COMPRESS`, `f'(x) > 0` and
/// `lim_{x→∞} f(x) = OVERFLOW_LIMIT`.
fn smooth_clamp(x: f64) -> f64 {
    debug_assert!(x >= 0.0);
    let limit = OVERFLOW_LIMIT;
    limit - limit / ((1.0 / OVERFLOW_COMPRESS * x) * (1.0 / limit) + 1.0)
}

/// The inverse function of `smooth_clamp`.
fn inverse_smooth_clamp(y: f64) -> f64 {
    let y = y.fmin(OVERFLOW_CLAMP);
    OVERFLOW_COMPRESS * y / (1.0 - y * (1.0 / OVERFLOW_LIMIT))
}

fn smooth_clamp_twoside(x: f64, range: std::ops::Range<f64>) -> f64 {
    if x <= range.start {
        range.start - smooth_clamp(range.start - x)
    } else if x >= range.end {
        range.end + smooth_clamp(x - range.end)
    } else {
        x
    }
}

fn inverse_smooth_clamp_twoside(y: f64, range: std::ops::Range<f64>) -> f64 {
    if y <= range.start {
        range.start - inverse_smooth_clamp(range.start - y)
    } else if y >= range.end {
        range.end + inverse_smooth_clamp(y - range.end)
    } else {
        y
    }
}

fn start_transition(
    hwnd: HWndRef,
    duration: f32,
    mut f: impl FnMut(pal::Wm, f32) -> bool + 'static,
) {
    use std::time::Instant;
    let start = Instant::now();

    start_animation_timer(hwnd, move |wm| {
        let elapsed = start.elapsed().as_secs_f32();
        let progress = (elapsed / duration).fmin(1.0);
        f(wm, progress)
    });
}

// TODO: This utility function is convenient and should be moved to something
//       like a utility module.
/// Call the given function each frame until it returns `false`. `hwnd` is used
/// to decide the display refresh rate to synchronize.
fn start_animation_timer(hwnd: HWndRef, f: impl FnMut(pal::Wm) -> bool + 'static) {
    struct TimerState<T: ?Sized>(T);

    impl<T: ?Sized + FnMut(pal::Wm) -> bool + 'static> TimerState<T> {
        fn enqueue(hwnd: HWndRef, mut this: Box<Self>) {
            hwnd.invoke_on_next_frame(move |wm, hwnd| {
                let keep_running = (this.0)(wm);

                if keep_running {
                    // Try to simulate `requestAnimationFrame`
                    Self::enqueue(hwnd, this);
                }
            });
        }
    }

    let st: Box<TimerState<dyn FnMut(pal::Wm) -> bool>> = Box::new(TimerState(f));

    TimerState::enqueue(hwnd, st);
}

/// Numerically solve an equation `f(x) = 0` using the Newton's method.
fn solve_newton(start: f32, mut f: impl FnMut(f32) -> f32, mut f_d: impl FnMut(f32) -> f32) -> f32 {
    (0..12).fold(start, |x, _| x - f(x) / f_d(x))
}

/// Evaluate a Bézier timing function. It's defined like a CSS timing function,
/// but the endpoints are different.
///
/// The bezier control points are: `(0, 0)`, `(x1, y1)`, `(x2, y2)`, and
/// `(1.0, y2)`.
fn eval_bezier_bouncing_timing_func(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let fn_t = |p| eval_cubic_bezier(p, x1, x2, 1.0);
    let fn_t_d = |p| eval_cubic_bezier_d(p, x1, x2, 1.0);

    let fn_y = |p| eval_cubic_bezier(p, y1, y2, y2);

    // All timing function we use have `fn_t` that is convex in range `[0, 1]`,
    // so assuming infinite precision, this initial guess guarantees convergence
    let p = solve_newton(1.0, |p| fn_t(p) - t, fn_t_d);

    fn_y(p)
}

#[inline]
fn eval_cubic_bezier(x: f32, y1: f32, y2: f32, y3: f32) -> f32 {
    // 3y1 * (1-x)²x + 3y2 * (1-x)x² + y3 * x³
    //  = 3y1(x-2x²+x³) + 3y2(x²-x³) + y3x³
    //  = x³(3(y1-y2)+y3) + x²(3(y2-y1)-3y1) + 3xy1
    x * (3.0 * y1 + x * ((3.0 * (y2 - y1) - 3.0 * y1) + x * (y3 - 3.0 * (y2 - y1))))
}

#[inline]
fn eval_cubic_bezier_d(x: f32, y1: f32, y2: f32, y3: f32) -> f32 {
    // (3y1 * (1-x)²x + 3y2 * (1-x)x² + y3 * x³)'
    //  = 3x²(3(y1-y2)+y3) + 2x(3(y2-y1)-3y1) + 3y1
    3.0 * y1 + x * ((3.0 * (y2 - y1) - 3.0 * y1) * 2.0 + 3.0 * x * (y3 - 3.0 * (y2 - y1)))
}

#[cfg(test)]
mod tests {
    use cggeom::box2;
    use cgmath::assert_abs_diff_eq;
    use log::debug;
    use quickcheck_macros::quickcheck;

    use super::*;
    use crate::{
        testing::{prelude::*, use_testing_wm},
        uicore::HWnd,
    };

    #[quickcheck]
    fn solve_newton_convergence(y1: f32, y: f32) -> bool {
        let y1 = y1.fract().abs();
        let y = y.fract().abs();
        debug!("(y1, y) = {:?}", (y1, y));

        let f = |x| eval_cubic_bezier(x, y1, 0.25, 1.0) - y;
        let f_d = |x| eval_cubic_bezier_d(x, y1, 0.25, 1.0);

        let x = solve_newton(1.0, &f, &f_d);
        debug!("x = {:?}, f(x) = {:?}", x, f(x));

        f(x).abs() < 1.0e-2
    }

    #[test]
    fn test_eval_bezier_bouncing_timing_func() {
        let (x1, y1) = (0.25, 10.0);
        assert_abs_diff_eq!(
            eval_bezier_bouncing_timing_func(x1, y1, 0.25, 0.0, 0.0),
            0.0,
            epsilon = 1.0e-3
        );
        assert_abs_diff_eq!(
            eval_bezier_bouncing_timing_func(x1, y1, 0.25, 0.0, 1.0),
            0.0,
            epsilon = 1.0e-3
        );

        assert_abs_diff_eq!(
            eval_bezier_bouncing_timing_func(0.2, 1.0, 0.0, 1.0, 1.0),
            1.0,
            epsilon = 1.0e-3
        );
        assert_abs_diff_eq!(
            eval_bezier_bouncing_timing_func(0.0, 1.0, 0.25, 1.0, 0.9),
            0.99824,
            epsilon = 1.0e-2
        );
    }

    struct TestModelState {
        value: Cell<Point2<f64>>,
        bounds: Cell<Box2<f64>>,
        orig: Cell<Point2<f64>>,
    }

    struct TestModel(Rc<TestModelState>);

    impl ScrollModel for TestModel {
        fn bounds(&mut self) -> Box2<f64> {
            self.0.bounds.get()
        }
        fn pos(&mut self) -> Point2<f64> {
            self.0.value.get()
        }
        fn set_pos(&mut self, value: Point2<f64>) {
            self.0.value.set(value);
        }
        fn cancel(&mut self) {
            self.0.value.set(self.0.orig.get());
        }
    }

    fn init_model_st() -> TestModelState {
        TestModelState {
            value: Cell::new([100.0, 100.0].into()),
            bounds: Cell::new(box2! {
                min: [50.0, 60.0],
                max: [200.0, 250.0],
            }),
            orig: Cell::new([80.0, 80.0].into()),
        }
    }

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn scroll_motion(twm: &dyn TestingWm) {
        let wm = twm.wm();

        let model_st = Rc::new(init_model_st());
        let model_getter_fac = || {
            let model_st = Rc::clone(&model_st);
            move || Box::new(TestModel(Rc::clone(&model_st))) as Box<dyn ScrollModel>
        };

        let expected_pos = model_st.value.get();
        let scrollable = ScrollWheelMixin::new();

        scrollable.scroll_motion(
            wm,
            &ScrollDelta {
                precise: true,
                delta: [-5.0, -10.0].into(),
            },
            model_getter_fac(),
        );
        twm.step_unsend();

        let expected_pos = expected_pos + Vector2::new(5.0, 10.0);
        assert_eq!(model_st.value.get(), expected_pos);

        scrollable.scroll_motion(
            wm,
            &ScrollDelta {
                precise: true,
                delta: [-1.0e6, -1.0e6].into(),
            },
            model_getter_fac(),
        );
        twm.step_unsend();

        let expected_pos = model_st.bounds.get().max;
        assert_eq!(model_st.value.get(), expected_pos);
    }

    fn wait_for(twm: &dyn TestingWm, ms: u64) {
        use std::time::{Duration, Instant};
        let till = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < till {
            twm.step_until(till);
        }
    }

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn no_momentum(twm: &dyn TestingWm) {
        let wm = twm.wm();
        let hwnd = HWnd::new(wm);
        hwnd.set_visibility(true);
        twm.step_unsend();
        let hview = hwnd.content_view();

        let model_st = Rc::new(init_model_st());
        let model_getter_fac = || {
            let model_st = Rc::clone(&model_st);
            move || Box::new(TestModel(Rc::clone(&model_st))) as Box<dyn ScrollModel>
        };

        let expected_pos = model_st.value.get();
        let scrollable = ScrollWheelMixin::new();
        let scroll = scrollable.scroll_gesture(model_getter_fac());

        scroll.motion(
            wm,
            hview.as_ref(),
            &ScrollDelta {
                precise: true,
                delta: [-5.0, -10.0].into(),
            },
            [0.0, 5.0].into(),
        );

        twm.step_unsend();
        wait_for(twm, 100);

        let expected_pos = expected_pos + Vector2::new(0.0, 10.0);
        assert_eq!(model_st.value.get(), expected_pos);

        scroll.end(wm, hview.as_ref());
        twm.step_unsend();
        wait_for(twm, 100);

        assert_eq!(model_st.value.get(), expected_pos);
    }

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn relaxation(twm: &dyn TestingWm) {
        let wm = twm.wm();
        let hwnd = HWnd::new(wm);
        hwnd.set_visibility(true);
        twm.step_unsend();
        let hview = hwnd.content_view();

        let model_st = Rc::new(init_model_st());
        let model_getter_fac = || {
            let model_st = Rc::clone(&model_st);
            move || Box::new(TestModel(Rc::clone(&model_st))) as Box<dyn ScrollModel>
        };

        let scrollable = ScrollWheelMixin::new();
        let scroll = scrollable.scroll_gesture(model_getter_fac());

        scroll.motion(
            wm,
            hview.as_ref(),
            &ScrollDelta {
                precise: true,
                delta: [0.0, -1.0e8].into(),
            },
            [0.0, 5.0].into(),
        );

        twm.step_unsend();
        wait_for(twm, 100);

        scroll.motion(
            wm,
            hview.as_ref(),
            &ScrollDelta {
                precise: true,
                delta: [-1.0e8, 0.0].into(),
            },
            [5.0, 0.0].into(),
        );

        twm.step_unsend();
        wait_for(twm, 100);

        use cggeom::BoolArray;
        let mut pos = model_st.value.get();
        assert!(pos.element_wise_gt(&model_st.bounds.get().max).all());
        assert!(pos.x < 1000.0 && pos.y < 1000.0);

        scroll.end(wm, hview.as_ref());

        for i in 0..100 {
            // `value` should decrease gradually until it reaches `max`
            let p = model_st.value.get();
            let max = model_st.bounds.get().max;

            debug!("{}: p = {:?}", i, p);

            if p.x <= max.x || p.y <= max.y {
                return;
            }
            assert!(p.x <= pos.x || p.y <= pos.y);

            pos = p;
            twm.step_unsend();
            wait_for(twm, 20);
        }

        panic!("The animation did not complete before a certain period of time.");
    }

    // TODO: somehow test the bounce animation
}
