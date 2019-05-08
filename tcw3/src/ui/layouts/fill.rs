use cggeom::{prelude::*, Box2};
use cgmath::{vec2, Point2, Vector2};

use crate::uicore::{HView, Layout, LayoutCtx, SizeTraits};

/// A `Layout` that overlaps a subview to fill the owning view.
#[derive(Debug, Clone)]
pub struct FillLayout {
    subview: [HView; 1],
    margin: f32,
}

impl FillLayout {
    /// Construct a `FillLayout` that fills the associated view with a
    /// specified view.
    pub fn new(subview: HView) -> Self {
        Self {
            subview: [subview],
            margin: 0.0,
        }
    }

    /// Construct a `FillLayout` that fills the associated view with a
    /// specified view using a specified margin value applied for all edges.
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

    fn has_same_subviews(&self, other: &dyn Layout) -> bool {
        if let Some(other) = as_any::Downcast::downcast_ref::<Self>(other) {
            self.subview[0] == other.subview[0]
        } else {
            false
        }
    }
}
