use alt_fp::FloatOrd;
use bitflags::bitflags;
use cggeom::{prelude::*, Box2};

use crate::uicore::SizeTraits;

bitflags! {
    #[cfg_attr(rustdoc, svgbobdoc::transform)]
    /// Describes how to align a layout element within the containing box.
    ///
    /// ```svgbob
    /// ,--------------------------------, ,--+-------------+--+--------------+--,
    /// |                                | |  |     TOP     |  |              |  |
    /// +------, ,-------------, ,-------+ |  '-------------'  |              |  |
    /// | LEFT | | HORZ_CENTER | | RIGHT | |                   |              |  |
    /// +------' '-------------' '-------+ |  ,-------------,  |              |  |
    /// |                                | |  | VERT_CENTER |  | VERT_JUSTIFY |  |
    /// +--------------------------------+ |  '-------------'  |              |  |
    /// |          HORZ_JUSTIFY          | |                   |              |  |
    /// +--------------------------------+ |  ,-------------,  |              |  |
    /// |                                | |  |    BOTTOM   |  |              |  |
    /// '--------------------------------' '--+-------------+--+--------------+--'
    /// ```
    pub struct AlignFlags: u8 {
        /// Align the element with the left edge.
        const LEFT = 0x1;
        /// Align the element with the right edge.
        const RIGHT = 0x2;
        /// Center the element horizontically.
        const HORZ_CENTER = 0;
        /// Justify the element horizontically.
        const HORZ_JUSTIFY = Self::LEFT.bits | Self::RIGHT.bits;

        /// The mask for the horizontal alignemnt flags.
        const HORZ_MASK = 0x3;

        /// Align the element with the top edge.
        const TOP = 0x10;
        /// Align the element with the bottom edge.
        const BOTTOM = 0x20;
        /// Center the element vertically.
        const VERT_CENTER = 0;
        /// Justify the element vertically.
        const VERT_JUSTIFY = Self::TOP.bits | Self::BOTTOM.bits;

        /// The mask for the vertical alignemnt flags.
        const VERT_MASK = 0x30;

        /// Center the element.
        const CENTER = Self::HORZ_CENTER.bits | Self::VERT_CENTER.bits;
        /// Justify the element.
        const JUSTIFY = Self::HORZ_JUSTIFY.bits | Self::VERT_JUSTIFY.bits;
    }
}

impl AlignFlags {
    /// Get `SizeTraits` for a containing box with the specified `AlignFlags`.
    pub(crate) fn containing_size_traits(self, mut content: SizeTraits) -> SizeTraits {
        if !self.contains(AlignFlags::HORZ_JUSTIFY) {
            content.max.x = std::f32::INFINITY;
        }
        if !self.contains(AlignFlags::VERT_JUSTIFY) {
            content.max.y = std::f32::INFINITY;
        }
        content
    }

    /// Arrange a layer box within the containing box based on `AlignFlags` and
    /// `SizeTraits`.
    pub(crate) fn arrange_child(self, container: &Box2<f32>, content: &SizeTraits) -> Box2<f32> {
        let mut child = *container;

        // The size when the child is not justified
        let size_x = content.preferred.x.fmin(container.size().x);
        let size_y = content.preferred.y.fmin(container.size().y);

        match self & AlignFlags::HORZ_JUSTIFY {
            x if x == AlignFlags::HORZ_CENTER => {
                let center = (container.min.x + container.max.x) * 0.5;
                child.min.x = center - size_x * 0.5;
                child.max.x = center + size_x * 0.5;
            }
            x if x == AlignFlags::LEFT => {
                child.max.x = child.min.x + size_x;
            }
            x if x == AlignFlags::RIGHT => {
                child.min.x = child.max.x - size_x;
            }
            x if x == AlignFlags::HORZ_JUSTIFY => {}
            _ => unreachable!(),
        }

        match self & AlignFlags::VERT_JUSTIFY {
            x if x == AlignFlags::VERT_CENTER => {
                let center = (container.min.y + container.max.y) * 0.5;
                child.min.y = center - size_y * 0.5;
                child.max.y = center + size_y * 0.5;
            }
            x if x == AlignFlags::TOP => {
                child.max.y = child.min.y + size_y;
            }
            x if x == AlignFlags::BOTTOM => {
                child.min.y = child.max.y - size_y;
            }
            x if x == AlignFlags::VERT_JUSTIFY => {}
            _ => unreachable!(),
        }

        child
    }
}
