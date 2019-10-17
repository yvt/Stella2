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

#[cfg(test)]
mod tests {
    use cggeom::box2;

    use super::*;
    use crate::{
        testing::{prelude::*, use_testing_wm},
        uicore::HWnd,
    };

    #[use_testing_wm(testing = "crate::testing")]
    #[test]
    fn test(twm: &dyn TestingWm) {
        let wm = twm.wm();

        let sv1 = HView::new(Default::default());

        let sv2 = HView::new(Default::default());
        sv2.set_layout(AbsLayout::new(
            SizeTraits {
                min: [20.0; 2].into(),
                max: [20.0; 2].into(),
                preferred: [20.0; 2].into(),
            },
            std::iter::empty(),
        ));

        let wnd = HWnd::new(wm);
        wnd.content_view().set_layout(AbsLayout::new(
            SizeTraits {
                min: [100.0; 2].into(),
                max: [100.0; 2].into(),
                preferred: [100.0; 2].into(),
            },
            vec![
                (
                    sv1.clone(),
                    box2! { min: [10.0; 2], max: [30.0; 2] },
                    AlignFlags::JUSTIFY,
                ),
                (
                    sv2.clone(),
                    box2! { min: [50.0; 2], max: [90.0; 2] },
                    AlignFlags::CENTER,
                ),
            ],
        ));
        wnd.set_visibility(true);
        twm.step_unsend();

        assert_eq!(sv1.global_frame(), box2! { min: [10.0; 2], max: [30.0; 2] });
        assert_eq!(sv2.global_frame(), box2! { min: [60.0; 2], max: [80.0; 2] });
    }
}
