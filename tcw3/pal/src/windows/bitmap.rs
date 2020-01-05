use cggeom::Box2;
use cgmath::{Matrix3, Point2};
use std::ptr::null_mut;
use winapi::um::{gdiplusinit, winnt::CHAR};

use super::CharStyleAttrs;
use crate::iface;

/// Call `GdiplusStartup` if it hasn't been called yet.
fn ensure_gdip_inited() {
    lazy_static::lazy_static! {
        static ref GDIP_INIT: () = {
            let input = gdiplusinit::GdiplusStartupInput::new(
                if log::STATIC_MAX_LEVEL == log::LevelFilter::Off {
                    None
                } else {
                    Some(gdip_debug_event_handler)
                },
                0, // do not suppress the GDI+ background thread
                1, // suppress external codecs
            );

            unsafe {
                gdiplusinit::GdiplusStartup(
                    // don't need a token, we won't call `GdiplusShutdown`
                    null_mut(),
                    &input,
                    // output is not necessary because we don't suppress the
                    // GDI+ background thread
                    null_mut(),
                );
            }
        };
    }

    let () = &*GDIP_INIT;

    extern "system" fn gdip_debug_event_handler(
        level: gdiplusinit::DebugEventLevel,
        message: *mut CHAR,
    ) {
        let level = match level {
            gdiplusinit::DebugEventLevelFatal => log::Level::Error,
            gdiplusinit::DebugEventLevelWarning => log::Level::Warn,
            _ => log::Level::Error,
        };

        log::log!(level, "GDI+ debug event: {:?}", unsafe {
            std::ffi::CStr::from_ptr(message)
        });
    }
}

#[derive(Debug, Clone)]
pub struct Bitmap;

impl iface::Bitmap for Bitmap {
    fn size(&self) -> [u32; 2] {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct BitmapBuilder;

impl iface::BitmapBuilderNew for BitmapBuilder {
    fn new(size: [u32; 2]) -> Self {
        ensure_gdip_inited();

        unimplemented!()
    }
}

impl iface::BitmapBuilder for BitmapBuilder {
    type Bitmap = Bitmap;

    fn into_bitmap(self) -> Self::Bitmap {
        unimplemented!()
    }
}

impl iface::Canvas for BitmapBuilder {
    fn save(&mut self) {
        unimplemented!()
    }
    fn restore(&mut self) {
        unimplemented!()
    }
    fn begin_path(&mut self) {
        unimplemented!()
    }
    fn close_path(&mut self) {
        unimplemented!()
    }
    fn move_to(&mut self, p: Point2<f32>) {
        unimplemented!()
    }
    fn line_to(&mut self, p: Point2<f32>) {
        unimplemented!()
    }
    fn cubic_bezier_to(&mut self, cp1: Point2<f32>, cp2: Point2<f32>, p: Point2<f32>) {
        unimplemented!()
    }
    fn quad_bezier_to(&mut self, cp: Point2<f32>, p: Point2<f32>) {
        unimplemented!()
    }
    fn fill(&mut self) {
        unimplemented!()
    }
    fn stroke(&mut self) {
        unimplemented!()
    }
    fn clip(&mut self) {
        unimplemented!()
    }
    fn set_fill_rgb(&mut self, rgb: iface::RGBAF32) {
        unimplemented!()
    }
    fn set_stroke_rgb(&mut self, rgb: iface::RGBAF32) {
        unimplemented!()
    }
    fn set_line_cap(&mut self, cap: iface::LineCap) {
        unimplemented!()
    }
    fn set_line_join(&mut self, join: iface::LineJoin) {
        unimplemented!()
    }
    fn set_line_dash(&mut self, phase: f32, lengths: &[f32]) {
        unimplemented!()
    }
    fn set_line_width(&mut self, width: f32) {
        unimplemented!()
    }
    fn set_line_miter_limit(&mut self, miter_limit: f32) {
        unimplemented!()
    }
    fn mult_transform(&mut self, m: Matrix3<f32>) {
        unimplemented!()
    }
}

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: iface::RGBAF32) {
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub struct CharStyle;

impl iface::CharStyle for CharStyle {
    fn new(attrs: CharStyleAttrs) -> Self {
        unimplemented!()
    }

    fn size(&self) -> f32 {
        unimplemented!()
    }
}

#[derive(Debug)]
pub struct TextLayout;

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        unimplemented!()
    }

    fn visual_bounds(&self) -> Box2<f32> {
        unimplemented!()
    }

    fn layout_bounds(&self) -> Box2<f32> {
        unimplemented!()
    }
}
