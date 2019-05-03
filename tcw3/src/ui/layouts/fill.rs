use cggeom::{Box2, prelude::*};
use cgmath::{vec2, Point2, Vector2};

use crate::uicore::{HView, Layout, LayoutCtx, SizeTraits};

/// A `Layout` that overlaps a subview to fill the owning view.
#[derive(Debug, Clone)]
pub struct FillLayout {
    subview: [HView; 1],
    margin: f32,
}

impl FillLayout {
    pub fn with_uniform_margin(subview: HView, margin: f32) -> Self {
        Self {
            subview: [subview],
            margin,
        }
    }
}

impl Layout for FillLayout {
    fn subviews(&self) -> &[HView] {
        &self.subview
    }

    fn size_traits(&self, ctx: &LayoutCtx<'_>) -> SizeTraits {
        let st = ctx.subview_size_traits(&self.subview[0]);
        let extra = vec2(self.margin, self.margin) * 2.0;
        SizeTraits {
            min: st.min + extra,
            max: st.max + extra,
            preferred: st.preferred + extra,
        }
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, size: Vector2<f32>) {
        ctx.set_subview_frame(
            &self.subview[0],
            Box2::new(
                Point2::new(self.margin, self.margin),
                Point2::new(size.x - self.margin, size.y - self.margin),
            ),
        );
    }
}
