use cggeom::box2;
use cgmath::{vec2, Vector2};

use crate::uicore::{HView, Layout, LayoutCtx, SizeTraits};

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
        let extra = vec2(
            self.margin[1] + self.margin[3],
            self.margin[0] + self.margin[2],
        );
        SizeTraits {
            min: st.min + extra,
            max: st.max + extra,
            preferred: st.preferred + extra,
        }
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        ctx.set_subview_frame(
            self.subview[0].as_ref(),
            box2! {
                min: [self.margin[3], self.margin[0]],
                max: [size.x - self.margin[1], size.y - self.margin[2]],
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
        assert_eq!(sv.global_frame(), box2! { min: [10.0; 2], max: [40.0; 2] });
        assert_eq!(
            wnd.content_view().global_frame(),
            box2! { min: [0.0; 2], max: [50.0; 2] }
        );
    }
}
