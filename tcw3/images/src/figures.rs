//! Creates `HImg` for basic figures.
use alt_fp::{FloatOrd, FloatOrdSet};
use cggeom::box2;
use packed_simd::{f32x4, shuffle};
use std::borrow::Borrow;
use tcw3_pal::{prelude::*, RGBAF32};

use super::{himg_from_paint_fn, HImg, PaintContext};

/// A drawing command for [`himg_from_figures`].
#[derive(Debug, Clone, Copy)]
pub struct Figure {
    color: RGBAF32,
    margins: [f32; 4],
    radii: [[f32; 2]; 4],
    line_width: f32,
}

impl PartialEq for Figure {
    /// Compare the fields by `==`. The exception is `line_width`, for which `eq`
    /// performs a bit-wise comparison (`a.to_bits() == b.to_bits()`) so that the
    /// comparison makes sense when `line_width` is not specified.
    fn eq(&self, other: &Self) -> bool {
        (
            self.color,
            self.margins,
            self.radii,
            self.line_width.to_bits(),
        ) == (
            other.color,
            other.margins,
            other.radii,
            other.line_width.to_bits(),
        )
    }
}

impl Figure {
    /// Construct a `Figure` representing a rectangle.
    pub const fn rect(color: RGBAF32) -> Self {
        Self {
            color,
            margins: [0.0; 4],
            radii: [[0.0; 2]; 4],
            line_width: std::f32::NAN,
        }
    }

    pub const fn with_corner_radius(self, radius: f32) -> Self {
        Self {
            radii: [[radius; 2]; 4],
            ..self
        }
    }

    pub const fn with_corner_radii(self, radii: [[f32; 2]; 4]) -> Self {
        Self { radii, ..self }
    }

    pub const fn with_margin(self, margins: [f32; 4]) -> Self {
        Self { margins, ..self }
    }

    /// Set the line width. `NAN` (default value) means the figure is filled.
    pub const fn with_line_width(self, line_width: f32) -> Self {
        Self { line_width, ..self }
    }
}

/// The specialization of `himg_from_figures` for a static slice. Ensures
/// there's only one instantiation of this specialization in the compiled
/// binary.
#[doc(hidden)]
#[inline(never)]
pub fn himg_from_figures_slice(figures: &'static [Figure]) -> HImg {
    himg_from_figures(figures)
}

/// Construct a `HImg` containing the specified list of figures.
pub fn himg_from_figures(figures: impl Borrow<[Figure]> + Send + Sync + 'static) -> HImg {
    // Calculate the maximum radius for each direction
    fn calc_size(figures: &[Figure]) -> [f32; 2] {
        let margins = figures
            .iter()
            .map(|fig| {
                let Figure {
                    radii,
                    margins,
                    line_width,
                    ..
                } = &fig;

                // Convert NaN to zero
                let line_width = line_width.fmax(0.0);

                f32x4::from(*margins)
                    + [
                        f32x4::new(radii[0][1], radii[1][0], radii[2][1], radii[3][0]),
                        f32x4::new(radii[1][1], radii[2][0], radii[3][1], radii[0][0]),
                    ]
                    .fmax()
                    + f32x4::splat(line_width)
            })
            .fold(f32x4::splat(0.0), FloatOrd::fmax);

        let size: f32x4 = [
            shuffle!(margins, [1, 0, 0, 0]),
            shuffle!(margins, [3, 2, 0, 0]),
        ]
        .fmax();
        let size = size * 2.0 + 2.0;

        [size.extract(0), size.extract(1)]
    }

    let size = calc_size(figures.borrow());

    // Construct `HImg`. The core routine is separated into a non-generic
    // function to reduce the code size.
    fn paint(figures: &[Figure], draw_ctx: &mut PaintContext<'_>) {
        let c = &mut draw_ctx.canvas;

        for figure in figures.iter() {
            let Figure { radii, margins, .. } = figure;

            let bx = box2! {
                min: [margins[3], margins[0]],
                max: [
                    draw_ctx.size[0] - margins[1],
                    draw_ctx.size[1] - margins[2],
                ]
            };

            c.rounded_rect(bx, *radii);
            if figure.line_width.is_nan() {
                c.set_fill_rgb(figure.color);
                c.fill();
            } else {
                c.set_stroke_rgb(figure.color);
                c.set_line_width(figure.line_width);
                c.stroke();
            }
        }
    }

    himg_from_paint_fn(size.into(), move |draw_ctx| {
        paint(figures.borrow(), draw_ctx)
    })
}

/// Construct a `HImg` containing a filled rounded rectangle.
pub fn himg_from_rounded_rect(color: RGBAF32, radii: [[f32; 2]; 4]) -> HImg {
    himg_from_figures([Figure::rect(color).with_corner_radii(radii)])
}

/// Create an array of [`Figure`]s.
///
/// # Examples
///
/// ```
/// use tcw3_images::{figures, Figure};
///
/// assert_eq!(
///     [
///         Figure::rect([1.0, 1.0, 1.0, 1.0].into())
///             .with_corner_radius(3.0),
///         Figure::rect([0.0, 0.0, 0.0, 1.0].into())
///             .with_corner_radius(2.0)
///             .with_margin([1.0; 4])
///             .with_line_width(2.0),
///     ],
///     figures![
///         rect([1.0, 1.0, 1.0, 1.0]).radius(3.0),
///         rect([0.0, 0.0, 0.0, 1.0])
///             .radius(2.0).margin([1.0; 4]).line_width(2.0),
///     ]
/// )
/// ```
#[macro_export]
macro_rules! figures {
    ($(
        $ctor:ident($($args1:tt)*)
        $(. $modify:ident($($args2:tt)*))*
    ),*$(,)*) => {[$(
        $crate::figures!(@figure $ctor($($args1)*) $(. $modify($($args2)*))*)
    ),*]};

    // Defining a `Figure`
    (@figure
        rect($($color:tt)*)
        $(. $modify:ident($($args:tt)*))*
    ) => {{
        let fig = $crate::Figure::rect($crate::figures!(@color $($color)*));
        $(
            let fig = $crate::figures!(@modifier $modify) (fig, $($args)*);
        )*
        fig
    }};

    (@modifier radius) => {$crate::Figure::with_corner_radius};
    (@modifier radii) => {$crate::Figure::with_corner_radii};
    (@modifier margin) => {$crate::Figure::with_margin};
    (@modifier line_width) => {$crate::Figure::with_line_width};
    (@modifier $unknown:ident) => {
        compile_error!(concat!("Unknown modifier: `", stringify!($unknown), "`"))
    };

    // Flexible color definition
    (@color [$r:expr, $g:expr, $b:expr, $a:expr]) => {
        $crate::RGBAF32::new($r, $g, $b, $a)
    };
    (@color $x:expr) => { $x };
}

/// Create a `HImg` from a static array of [`Figure`]s.
///
/// # Examples
///
/// ```
/// use tcw3_images::{himg_figures, HImg};
///
/// # fn no_run() {
/// let _: HImg = himg_figures![
///     rect([1.0, 1.0, 1.0, 1.0]).radius(3.0),
///     rect([0.0, 0.0, 0.0, 1.0]).radius(2.0).margin([1.0; 4]),
/// ];
/// # }
/// ```
#[macro_export]
macro_rules! himg_figures {
    ($($args:tt)*) => {{
        const FIGURES: &[$crate::Figure] = &$crate::figures![$($args)*];
        $crate::himg_from_figures_slice(FIGURES)
    }}
}
