use cggeom::ElementWiseOp;
use cgmath::Vector2;
use momo::momo;

use crate::{
    ui::layouts::EmptyLayout,
    uicore::{HView, SizeTraits, ViewFlags},
};

/// Construct a spacer widget, which has size traits but no graphical contents.
pub fn new_spacer(size_traits: SizeTraits) -> HView {
    let view = HView::new(ViewFlags::default());
    view.set_layout(EmptyLayout::new(size_traits));
    view
}

/// The builder for a spacer widget, which has size traits but no graphical
/// contents. This is an ergonomic wrapper for [`new_spacer`] and [`SizeTraits`].
///
/// [`SizeTraits`]: crate::uicore::SizeTraits
///
/// # Examples
///
/// ```
/// use tcw3::ui::views::Spacer;
/// let view = Spacer::new().with_min([4.0, 0.0]).into_view();
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct Spacer {
    size_traits: SizeTraits,
}

impl Spacer {
    /// Construst a `Spacer`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Update `SizeTraits::min` and return a new `Spacer`, consuming `self`.
    pub fn with_min(self, min: impl Into<Vector2<f32>>) -> Self {
        Self {
            size_traits: self.size_traits.with_min(min.into()),
        }
    }

    /// Update `SizeTraits::max` and return a new `Spacer`, consuming `self`.
    pub fn with_max(self, max: impl Into<Vector2<f32>>) -> Self {
        Self {
            size_traits: self.size_traits.with_max(max.into()),
        }
    }

    /// Update `SizeTraits::preferred` and return a new `Spacer`, consuming `self`.
    pub fn with_preferred(self, preferred: impl Into<Vector2<f32>>) -> Self {
        Self {
            size_traits: self.size_traits.with_preferred(preferred.into()),
        }
    }

    /// Update `SizeTraits::{min, max, preferred}` and return a new `Spacer`,
    /// consuming `self`.
    #[momo]
    pub fn with_fixed(self, size: impl Into<Vector2<f32>>) -> Self {
        Self {
            size_traits: SizeTraits {
                min: size,
                max: size,
                preferred: size,
            },
        }
    }

    /// Create a `HView`, consuming `self`.
    pub fn into_view(self) -> HView {
        new_spacer(SizeTraits {
            preferred: self
                .size_traits
                .preferred
                .element_wise_max(&self.size_traits.min),
            ..self.size_traits
        })
    }
}
