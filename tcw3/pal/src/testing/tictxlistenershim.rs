use cggeom::Box2;
use cgmath::Point2;
use std::ops::Range;

use super::{native, HTextInputCtx, HTextInputCtxInner, Wm};
use crate::iface;

/// Wraps `TextInputCtxListener<Wm>` to create a `TextInputCtxListener<native::Wm>`.
pub struct NativeTextInputCtxListener(pub Box<dyn iface::TextInputCtxListener<Wm>>);

fn from_native_htictx(htictx: &native::HTextInputCtx) -> HTextInputCtx {
    HTextInputCtx {
        inner: HTextInputCtxInner::Native(htictx.clone()),
    }
}

/// Argument conversion
macro_rules! forward_arg {
    ([htictx: $x:expr]) => {
        &from_native_htictx($x)
    };
    ([wm: $x:expr]) => {
        Wm::from_native_wm($x)
    };
    ($x:ident) => {
        $x
    };
}

/// Forward a method call to an inner type by converting arguments using
/// `forward_arg`.
macro_rules! forward {
    ($inner:expr, $method:ident $(, $arg:tt)*$(,)*) => {
        $inner.$method($(forward_arg!($arg)),*)
    };
}

impl iface::TextInputCtxListener<native::Wm> for NativeTextInputCtxListener {
    fn edit(
        &self,
        wm: native::Wm,
        htictx: &native::HTextInputCtx,
        mutating: bool,
    ) -> Box<dyn iface::TextInputCtxEdit<native::Wm>> {
        let edit = forward!(self.0, edit, [wm: wm], [htictx: htictx], mutating);

        Box::new(NativeTextInputCtxEdit(edit))
    }

    fn set_event_mask(
        &self,
        wm: native::Wm,
        htictx: &native::HTextInputCtx,
        flags: iface::TextInputCtxEventFlags,
    ) {
        forward!(self.0, set_event_mask, [wm: wm], [htictx: htictx], flags)
    }
}

/// Wraps `TextInputCtxEdit<Wm>` to create a `TextInputCtxEdit<native::Wm>`.
struct NativeTextInputCtxEdit(Box<dyn iface::TextInputCtxEdit<Wm>>);

impl iface::TextInputCtxEdit<native::Wm> for NativeTextInputCtxEdit {
    fn selected_range(&mut self) -> Range<usize> {
        forward!(self.0, selected_range)
    }

    fn set_selected_range(&mut self, range: Range<usize>) {
        forward!(self.0, set_selected_range, range)
    }

    fn set_composition_range(&mut self, range: Option<Range<usize>>) {
        forward!(self.0, set_composition_range, range)
    }

    fn replace(&mut self, range: Range<usize>, text: &str) {
        forward!(self.0, replace, range, text)
    }

    fn slice(&mut self, range: Range<usize>) -> String {
        forward!(self.0, slice, range)
    }

    fn len(&mut self) -> usize {
        forward!(self.0, len)
    }

    fn index_from_point(
        &mut self,
        point: Point2<f32>,
        flags: iface::IndexFromPointFlags,
    ) -> Option<usize> {
        forward!(self.0, index_from_point, point, flags)
    }

    fn frame(&mut self) -> Box2<f32> {
        forward!(self.0, frame)
    }

    fn slice_bounds(&mut self, range: Range<usize>) -> (Box2<f32>, usize) {
        forward!(self.0, slice_bounds, range)
    }
}
