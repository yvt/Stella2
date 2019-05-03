use cgmath::Vector2;

use crate::uicore::{HView, Layout, LayoutCtx, SizeTraits};

/// A `Layout` that doesn't have subviews.
#[derive(Debug, Clone)]
pub struct EmptyLayout {
    size_traits: SizeTraits,
}

impl EmptyLayout {
    pub fn new(size_traits: SizeTraits) -> Self {
        Self { size_traits }
    }
}

impl Layout for EmptyLayout {
    fn subviews(&self) -> &[HView] {
        &[]
    }

    fn size_traits(&self, _: &LayoutCtx<'_>) -> SizeTraits {
        self.size_traits
    }

    fn arrange(&self, _: &mut LayoutCtx<'_>, _: Vector2<f32>) {}
}
