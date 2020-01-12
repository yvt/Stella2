use cggeom::Box2;

use crate::iface;

#[derive(Debug, Clone)]
pub struct CharStyle {}

pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

impl iface::CharStyle for CharStyle {
    fn new(attrs: CharStyleAttrs) -> Self {
        log::warn!("CharStyle::new: stub!");
        Self {}
    }

    fn size(&self) -> f32 {
        12.0 // TODO
    }
}

#[derive(Debug)]
pub struct TextLayout {}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        log::warn!("TextLayout::new: stub!");
        Self {}
    }

    fn visual_bounds(&self) -> Box2<f32> {
        cggeom::box2! { min: [0.0, 0.0], max: [40.0, 10.0] } // TODO
    }

    fn layout_bounds(&self) -> Box2<f32> {
        cggeom::box2! { min: [0.0, 0.0], max: [40.0, 10.0] } // TODO
    }
}
