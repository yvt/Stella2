use cgmath::Point2;

use super::super::text::TextLayout;
use super::BitmapBuilder;
use crate::iface;

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: iface::RGBAF32) {
        log::warn!("BitmapBuilder::draw_text: stub!");
    }
}
