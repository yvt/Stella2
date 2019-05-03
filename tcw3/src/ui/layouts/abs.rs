use cggeom::Box2;
use cgmath::Vector2;

use crate::uicore::{HView, Layout, LayoutCtx, SizeTraits};

/// An absolutely-positioned `Layout`.
#[derive(Debug, Clone)]
pub struct AbsLayout {
    size_traits: SizeTraits,
    subviews: Vec<HView>,
    frames: Vec<Box2<f32>>,
}

impl AbsLayout {
    pub fn new(
        size_traits: SizeTraits,
        subviews: impl IntoIterator<Item = (HView, Box2<f32>)>,
    ) -> Self {
        let (subviews, frames) = subviews.into_iter().unzip();
        Self {
            size_traits,
            subviews,
            frames,
        }
    }
}

impl Layout for AbsLayout {
    fn subviews(&self) -> &[HView] {
        &self.subviews
    }

    fn size_traits(&self, _: &LayoutCtx<'_>) -> SizeTraits {
        self.size_traits
    }

    fn arrange(&self, ctx: &mut LayoutCtx<'_>, _: Vector2<f32>) {
        for (subview, frame) in self.subviews.iter().zip(self.frames.iter()) {
            ctx.set_subview_frame(subview, *frame);
        }
    }
}
