use cggeom::Box2;
use cgmath::Vector2;

use crate::{
    ui::AlignFlags,
    uicore::{HView, Layout, LayoutCtx, SizeTraits},
};

/// An absolutely-positioned `Layout`.
#[derive(Debug, Clone)]
pub struct AbsLayout {
    size_traits: SizeTraits,
    subviews: Vec<HView>,
    items: Vec<(Box2<f32>, AlignFlags)>,
}

impl AbsLayout {
    pub fn new(
        size_traits: SizeTraits,
        items: impl IntoIterator<Item = (HView, Box2<f32>, AlignFlags)>,
    ) -> Self {
        let (subviews, items) = items
            .into_iter()
            .map(|(view, frame, align)| (view, (frame, align)))
            .unzip();
        Self {
            size_traits,
            subviews,
            items,
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
        for (subview, (frame, align)) in self.subviews.iter().zip(self.items.iter()) {
            let st = ctx.subview_size_traits(subview);

            let subview_frame = align.arrange_child(frame, &st);

            ctx.set_subview_frame(subview, subview_frame);
        }
    }
}
