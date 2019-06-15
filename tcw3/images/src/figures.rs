//! Creates `HImg` for basic figures.
use alt_fp::FloatOrdSet;
use array::Array4;
use cggeom::box2;
use tcw3_pal::{prelude::*, RGBAF32};

use super::{himg_from_paint_fn, HImg};

/// Construct a `HImg` containing a filled rounded rectangle.
pub fn himg_from_rounded_rect(color: RGBAF32, radii: [[f32; 2]; 4]) -> HImg {
    // Calcualte the maximum radius for each direction
    let size = [
        radii.map(|r| r[0]).fmax() * 2.0 + 1.0,
        radii.map(|r| r[1]).fmax() * 2.0 + 1.0,
    ];

    himg_from_paint_fn(size.into(), move |draw_ctx| {
        let c = &mut draw_ctx.canvas;
        c.set_fill_rgb(color);

        c.rounded_rect(box2! { top_left: [0.0, 0.0], size: draw_ctx.size }, radii);
        c.fill();
    })
}
