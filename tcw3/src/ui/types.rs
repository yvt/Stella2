use alt_fp::FloatOrd;
use bitflags::bitflags;
use cggeom::{prelude::*, Box2};
use std::{cell::Cell, fmt};

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

/// Provides a counter, which is used to temporarily prevent updates when the
/// current value is greater than zero.
///
/// # Examples
///
/// ```
/// use {std::cell::Cell, tcw3::ui::{SuspendFlag, Suspend}};
/// #[derive(Default)]
/// struct Component {
///     dirty: Cell<bool>,
///     suspend_flag: SuspendFlag,
/// }
///
/// impl Component {
///     fn set_something(&self) {
///         // do something...
///         self.dirty.set(true);
///         self.flush_changes();
///     }
///
///     fn flush_changes(&self) {
///         if !self.suspend_flag.is_suspended() {
///             // do expensive things...
///             self.dirty.set(false);
///         }
///     }
///
///     fn suspend_update<'a>(&'a self) -> impl Suspend + 'a {
///         self.suspend_flag.suspend(move || { self.flush_changes(); })
///     }
/// }
/// let comp = Component::default();
///
/// // This immediately causes update:
/// comp.set_something();
/// assert_eq!(comp.dirty.get(), false);
///
/// // This defers update:
/// {
///     let _guard = comp.suspend_update();
///     comp.set_something();
///     assert_eq!(comp.dirty.get(), true);
/// };
/// assert_eq!(comp.dirty.get(), false);
/// ```
#[derive(Debug)]
pub struct SuspendFlag {
    count: Cell<usize>,
}

impl Default for SuspendFlag {
    fn default() -> Self {
        Self::new()
    }
}

impl SuspendFlag {
    /// Construct a `SuspendFlag`.
    pub fn new() -> Self {
        Self {
            count: Cell::new(0),
        }
    }

    /// Returns `true` if the current value is not zero.
    pub fn is_suspended(&self) -> bool {
        self.count.get() > 0
    }

    /// Increase the counter. Returns an RAII guard, which decrements the
    /// counter when dropped. When the value returns to zero, the specified
    /// function `resume` is called.
    pub fn suspend<F: FnOnce()>(&self, resume: F) -> SuspendGuard<'_, F> {
        self.count.set(self.count.get() + 1);
        SuspendGuard {
            flag: self,
            resume: Some(resume),
        }
    }
}

/// The marker trait for [`SuspendGuard`].
///
/// [`SuspendFlag::suspend`] returns a `SuspendGuard` with an indescribable
/// generic parameter. However, `impl` requires you to specify at least one
/// trait. The solution is this trait, which directly leads to here on the API
/// documentation.
///
/// Do not implement this trait for other types as doing so defeats the purpose
/// of the trait.
///
/// See [`SuspendFlag`] for the usage.
pub trait Suspend: fmt::Debug {}

/// A RAII guard for [`SuspendFlag`], which automatically decrements the counter
/// when dropped.
pub struct SuspendGuard<'a, T: FnOnce()> {
    flag: &'a SuspendFlag,
    resume: Option<T>,
}

impl<T: FnOnce()> Suspend for SuspendGuard<'_, T> {}

impl<T: FnOnce()> fmt::Debug for SuspendGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SuspendGuard")
            .field("flag", &self.flag)
            .field("resume", &())
            .finish()
    }
}

impl<T: FnOnce()> Drop for SuspendGuard<'_, T> {
    fn drop(&mut self) {
        let count = self.flag.count.get();
        self.flag.count.set(count - 1);
        if count == 1 {
            self.resume.take().unwrap()();
        }
    }
}
