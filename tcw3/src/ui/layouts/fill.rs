use alt_fp::FloatOrd;
use cggeom::box2;
use cgmath::Vector2;
use packed_simd::{f32x4, shuffle};

use crate::uicore::{HView, Layout, LayoutCtx, SizeTraits};

macro_rules! shuf_f32x4 {
    ($($tt:tt)*) => {{
        let x: f32x4 = shuffle!($($tt)*);
        x
    }};
}

/// A `Layout` that overlaps a subview to fill the owning view.
#[derive(Debug, Clone)]
pub struct FillLayout {
    subview: [HView; 1],
    margin: [f32; 4],
}

impl FillLayout {
    /// Construct a `FillLayout` that fills the associated view with a
    /// specified view.
    pub fn new(subview: HView) -> Self {
        Self {
            subview: [subview],
            margin: [0.0; 4],
        }
    }

    /// Construct a `FillLayout` that fills the associated view with a
    /// specified view using a specified margin value applied for all edges
    /// based on `self`, consuming `self`.
    pub fn with_uniform_margin(self, margin: f32) -> Self {
        Self {
            margin: [margin; 4],
            ..self
        }
    }

    /// Construct a `FillLayout` that fills the associated view with a
    /// specified view using separately specified margin values applied for
    /// corresponding edges based on `self`, consuming `self`.
    ///
    /// `NAN` means the margin is unspecified. If two margins facing at each
    /// other are both `NAN`, the view is centered along the corresponding axis.
    pub fn with_margin(self, margin: [f32; 4]) -> Self {
        Self { margin, ..self }
    }
}

impl Layout for FillLayout {
    fn subviews(&self) -> &[HView] {
        &self.subview
    }

    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits {
        let st = ctx.subview_size_traits(self.subview[0].as_ref());

        let margin: f32x4 = self.margin.into();

        // Unspecified edges (NaN)
        let margin_unspec = margin.eq(margin);

        // [top, right, bottom, left]
        let margin_min = margin_unspec.select(margin, f32x4::splat(0.0));
        let margin_max = margin_unspec.select(margin, f32x4::splat(f32::INFINITY));

        // [extra_min_x, extra_min_y, extra_max_x, extra_max_y]
        // extra_min_x = margin_min[1] + margin_min[3]
        //             = margin_min.left + margin_min.right
        let extra = shuf_f32x4!(margin_min, margin_max, [1, 0, 5, 4])
            + shuf_f32x4!(margin_min, margin_max, [3, 2, 7, 6]);

        // [st.min.x, st.min.y, st.max.x, st.max.y]
        let st_min_max = f32x4::new(st.min.x, st.min.y, st.max.x, st.max.y);
        // [st.preferred.x, st.preferred.y, ?, ?]
        let st_preferred = f32x4::new(st.preferred.x, st.preferred.y, 0.0, 0.0);

        let new_st_min_max: f32x4 = extra + st_min_max;
        let new_st_preferred: f32x4 = extra + st_preferred;

        SizeTraits {
            min: [new_st_min_max.extract(0), new_st_min_max.extract(1)].into(),
            max: [new_st_min_max.extract(2), new_st_min_max.extract(3)].into(),
            preferred: [new_st_preferred.extract(0), new_st_preferred.extract(1)].into(),
        }
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        let st = ctx.subview_size_traits(self.subview[0].as_ref());
        let mut margin = self.margin;

        // See `size_traits`
        let margin_f32x4: f32x4 = margin.into();
        let margin_unspec = margin_f32x4.eq(margin_f32x4);
        let margin_min = margin_unspec.select(margin_f32x4, f32x4::splat(0.0));

        // [extra_min_x, extra_min_y, ?, ?]
        let extra = shuf_f32x4!(margin_min, [1, 0, 1, 0]) + shuf_f32x4!(margin_min, [3, 2, 3, 2]);

        let margin_unspec_u8 = margin_unspec.bitmask();

        let mut flex_margin = [0.0; 2];

        // Assuming the flexible margins are all set to zero, `max_view_size` is
        // the size of the subview.
        let max_view_size = f32x4::new(size.x, size.y, 0.0, 0.0) - extra;
        let constrained_max_view_size = max_view_size
            .fmax(f32x4::new(st.min.x, st.min.y, 0.0, 0.0))
            .fmin(f32x4::new(st.max.x, st.max.y, 0.0, 0.0));
        let flex_margin_if_flexible = max_view_size - constrained_max_view_size;

        // Is the layout flexible in the horizontal direction?
        if margin_unspec_u8 & 0b1010 != 0 {
            flex_margin[0] = flex_margin_if_flexible.extract(0);

            // If both of the left and right margins are flexible, distribute
            // the adjustment evenly
            if (margin_unspec_u8 & 0b1010) == 0b1010 {
                flex_margin[0] /= 2.0;
            }
        }

        // Is the layout flexible in the vertical direction?
        if margin_unspec_u8 & 0b0101 != 0 {
            flex_margin[1] = flex_margin_if_flexible.extract(1);

            // If both of the top and bottom margins are flexible, distribute
            // the adjustment evenly
            if (margin_unspec_u8 & 0b0101) == 0b0101 {
                flex_margin[1] /= 2.0;
            }
        }

        // Recalculate margin
        margin_unspec
            .select(
                margin_f32x4,
                f32x4::new(
                    flex_margin[1],
                    flex_margin[0],
                    flex_margin[1],
                    flex_margin[0],
                ),
            )
            .write_to_slice_unaligned(&mut margin);

        ctx.set_subview_frame(
            self.subview[0].as_ref(),
            box2! {
                min: [margin[3], margin[0]],
                max: [size.x - margin[1], size.y - margin[2]],
            },
        );
    }

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        if let Some(other) = as_any::Downcast::downcast_ref::<Self>(other) {
            self.subview[0] == other.subview[0]
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use cggeom::box2;

    use super::*;
    use crate::{
        testing::{prelude::*, use_testing_wm},
        ui::layouts::EmptyLayout,
        uicore::HWnd,
    };

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn test(twm: &dyn TestingWm) {
        let wm = twm.wm();

        let sv = HView::new(Default::default());
        sv.set_layout(EmptyLayout::new(SizeTraits {
            min: [20.0; 2].into(),
            max: [50.0; 2].into(),
            preferred: [30.0; 2].into(),
        }));

        let wnd = HWnd::new(wm);
        wnd.content_view()
            .set_layout(FillLayout::new(sv.clone()).with_uniform_margin(10.0));
        wnd.set_visibility(true);
        twm.step_unsend();

        // preferred size
        assert_eq!(
            sv.global_frame(),
            box2! { min: [10.0, 10.0], max: [40.0, 40.0] }
        );
        assert_eq!(
            wnd.content_view().global_frame(),
            box2! { min: [0.0, 0.0], max: [50.0, 50.0] }
        );
    }
}
