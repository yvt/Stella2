use alt_fp::FloatOrd;

/// Evaluate a piecewise linear function for the specified input value.
///
/// `points` is a list of points specifying the function. Each element `(x, y)`
/// specifies that given an input value `x`, the output is `y`. `points` must be
/// sorted by `x` and must have at least one element.
///
/// For the region outside the domain of `points`, the values of the first and
/// last points in `points` are used as the output.
pub fn piecewise_map(points: impl IntoIterator<Item = (f64, f64)>, x: f64) -> f64 {
    let mut points = points.into_iter();

    let mut p1 = points.next().unwrap();
    if x <= p1.0 {
        return p1.1;
    }

    for p2 in points {
        // Must be `<` to guarantee exactness at `p2.0`
        if x < p2.0 {
            let frac = (x - p1.0) / (p2.0 - p1.0);

            let y = p1.1 + (p2.1 - p1.1) * frac;

            // guarantee monotonicity near `p2.0`
            return y.fmax(p1.1.fmin(p2.1)).fmin(p1.1.fmax(p2.1));
        }
        p1 = p2;
    }

    p1.1
}

/// The inverse function of `piecewise_map`.
///
/// See `piecewise_map` for the definition of the parameters. `points` must be
/// sorted by `y` instead.
pub fn piecewise_unmap(points: impl IntoIterator<Item = (f64, f64)>, y: f64) -> f64 {
    piecewise_map(points.into_iter().map(|(x, y)| (y, x)), y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use std::cmp::Ordering;

    fn adjust_by_ulp(x: f64, i: i64) -> f64 {
        assert!(x.is_finite());
        if x == 0.0 {
            if i >= 0 {
                <f64>::from_bits(i as u64)
            } else {
                -<f64>::from_bits((-i) as u64)
            }
        } else if x > 0.0 {
            let new_val = x.to_bits() as i64 + i;
            if new_val >= 0 {
                <f64>::from_bits(new_val as u64)
            } else {
                -<f64>::from_bits((-new_val) as u64)
            }
        } else if x < 0.0 {
            -adjust_by_ulp(-x, -i)
        } else {
            unreachable!();
        }
    }

    fn run_test(points: &[(f64, f64)]) {
        use std::f64::{INFINITY, NEG_INFINITY};
        let first = points.first().unwrap();
        let last = points.last().unwrap();
        let it = || points.iter().cloned();

        // Extrapolation
        assert_eq!(piecewise_map(it(), NEG_INFINITY), first.1);
        assert_eq!(piecewise_map(it(), first.0 - 1.0), first.1);
        assert_eq!(piecewise_map(it(), last.0 + 1.0), last.1);
        assert_eq!(piecewise_map(it(), INFINITY), last.1);

        // Exactness
        for &(x, y) in points.iter() {
            assert_eq!(piecewise_map(it(), x), y);
        }

        // Monotonicity
        const LEN: usize = 16;
        let mut xs = [0.0f64; LEN];
        let mut ys = xs.clone();
        for win in points.windows(2) {
            let x1 = win[0].0;
            let x2 = adjust_by_ulp(win[0].0, 1).fmin(win[1].0);
            let x3 = adjust_by_ulp(win[1].0, -1).fmax(x2);
            let x4 = win[1].0;
            xs[0] = x1;
            xs[1] = x2;
            xs[LEN - 2] = x3;
            xs[LEN - 1] = x4;
            for (i, x) in xs[2..LEN - 2].iter_mut().enumerate() {
                let x1 = x2 + (x3 - x2) * (i as f64 / (LEN - 4) as f64);
                let x1 = x1.fmin(x3);
                *x = x1;
            }

            for (x, y) in xs.iter_mut().zip(ys.iter_mut()) {
                *y = piecewise_map(it(), *x);
            }

            dbg!((&win, &xs, &ys));
            for ys in ys.windows(2) {
                match win[0].1.partial_cmp(&win[1].1).unwrap() {
                    Ordering::Equal => {
                        assert!(ys[0] == ys[1]);
                    }
                    Ordering::Greater => {
                        assert!(ys[0] >= ys[1]);
                    }
                    Ordering::Less => {
                        assert!(ys[0] <= ys[1]);
                    }
                }
            }
        }
    }

    #[test]
    fn one() {
        run_test(&[(1.0, 2.0)]);
    }

    #[test]
    fn two() {
        let nums = [-1.0e+100, -1.0, -4.0e-100, 5.0e-100, 42.0, 1.0e+100];

        for quadruple in (0..4)
            .map(|_| nums.iter().cloned())
            .multi_cartesian_product()
        {
            if quadruple[0] >= quadruple[2] {
                continue;
            }
            run_test(&[(quadruple[0], quadruple[1]), (quadruple[2], quadruple[3])]);
        }
    }

    #[test]
    fn many() {
        run_test(&[
            (0.0, 1.0),
            (1.0, 4.0),
            (2.0, -3.0),
            (3.0, 7.0),
            (4.0, 1.0),
            (5.0, 10.0),
        ]);
    }
}
