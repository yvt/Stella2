//! Fixed-point arithmetics for line size calculation.
//!
//! Line sizes use fixed-point numbers to prevent floating-point error
//! accumulation during the operation of a lineset. This module provides
//! functions for conversion between fixed-point numbers (used by `Lineset`) and
//! floating-point numbers (used by `Table`'s public interface).

const FACTOR: f64 = 16.0;

pub fn fp_to_fix(x: f64) -> i64 {
    (x * FACTOR) as i64
}

pub fn fix_to_fp(x: i64) -> f64 {
    x as f64 * (1.0 / FACTOR)
}

pub fn fix_to_f32(x: i64) -> f32 {
    x as f32 * (1.0 / FACTOR as f32)
}
