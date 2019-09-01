use cggeom::{box2, Box2};
use pango::{Context, FontDescription, Layout};
use rgb::RGBA16;

use super::super::iface;
use super::CharStyleAttrs;

#[derive(Debug, Clone)]
pub struct CharStyle {
    pango_font_desc: ImmutableFontDesc,
    decor: iface::TextDecorFlags,
    color: Option<RGBA16>,
}

#[derive(Debug, Clone)]
struct ImmutableFontDesc {
    inner: FontDescription,
}

// I think `FontDescription`'s thread unsafety comes from mutability
// TODO: Fact-check this
unsafe impl Send for ImmutableFontDesc {}
unsafe impl Sync for ImmutableFontDesc {}

impl iface::CharStyle for CharStyle {
    fn new(mut attrs: CharStyleAttrs) -> Self {
        let mut font_desc = FontDescription::new();

        if attrs.template.is_none() {
            // Set default values
            use iface::SysFontType;

            if attrs.sys.is_none() {
                attrs.sys = Some(iface::SysFontType::Normal);
            }

            if attrs.size.is_none() {
                attrs.size = Some(match attrs.sys.unwrap() {
                    SysFontType::Normal
                    | SysFontType::Emph
                    | SysFontType::User
                    | SysFontType::UserMonospace => 12.0,
                    SysFontType::Small | SysFontType::SmallEmph => 10.0,
                });
            }

            match attrs.sys.unwrap() {
                SysFontType::Normal | SysFontType::Small | SysFontType::User => {}
                SysFontType::Emph => {
                    font_desc.set_weight(pango::Weight::Bold);
                }
                SysFontType::UserMonospace => {
                    font_desc.set_family_static("Monospace");
                }
                SysFontType::SmallEmph => {
                    font_desc.set_weight(pango::Weight::Bold);
                }
            }
        }

        if let Some(size) = attrs.size {
            font_desc.set_size((size * pango::SCALE as f32) as i32);
        }

        let mut color = attrs.color.unwrap_or(None).map(rgbaf32_to_rgba16);

        let mut decor = attrs.decor.unwrap_or(iface::TextDecorFlags::empty());

        if let Some(tmpl) = attrs.template {
            font_desc.merge(Some(&tmpl.pango_font_desc.inner), false);
            color = tmpl.color;
            decor = tmpl.decor;
        }

        Self {
            pango_font_desc: ImmutableFontDesc { inner: font_desc },
            color,
            decor,
        }
    }

    fn size(&self) -> f32 {
        self.pango_font_desc.inner.get_size() as f32 * (1.0 / pango::SCALE as f32)
    }
}

fn rgbaf32_to_rgba16(c: iface::RGBAF32) -> RGBA16 {
    use rgb::ComponentMap;

    c.map(|x| (x * 65535.0) as u16)
}

#[derive(Debug)]
pub struct TextLayout {
    pango_layout: ImmutableLayout,
}

#[derive(Debug, Clone)]
struct ImmutableLayout {
    inner: Layout,
}

// I think `Layout`'s thread unsafety comes from mutability
// TODO: Fact-check this
unsafe impl Send for ImmutableLayout {}
unsafe impl Sync for ImmutableLayout {}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        unimplemented!()
    }

    fn visual_bounds(&self) -> Box2<f32> {
        let (ink_rect, _logical_rect) = self.pango_layout.inner.get_extents();
        pango_rect_to_box2_f32(ink_rect)
    }

    fn layout_bounds(&self) -> Box2<f32> {
        let (_ink_rect, logical_rect) = self.pango_layout.inner.get_extents();
        pango_rect_to_box2_f32(logical_rect)
    }
}

fn pango_rect_to_box2_f32(x: pango::Rectangle) -> Box2<f32> {
    let scale = pango::SCALE as f32;
    box2! {
        top_left: [x.x as f32 / scale, x.y as f32 / scale],
        size: [x.width as f32 / scale, x.height as f32 / scale],
    }
}
