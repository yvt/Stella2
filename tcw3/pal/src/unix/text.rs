use alt_fp::FloatOrd;
use cggeom::{box2, Box2};
use pango::{FontDescription, FontMapExt, Layout};
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
                SysFontType::Normal
                | SysFontType::Small
                | SysFontType::User
                | SysFontType::UserMonospace => {}
                SysFontType::Emph | SysFontType::SmallEmph => {
                    font_desc.set_weight(pango::Weight::Bold);
                }
            }

            match attrs.sys.unwrap() {
                SysFontType::Normal
                | SysFontType::Small
                | SysFontType::User
                | SysFontType::Emph
                | SysFontType::SmallEmph => {
                    font_desc.set_family_static("Sans");
                }
                SysFontType::UserMonospace => {
                    font_desc.set_family_static("Monospace");
                }
            }
        }

        if let Some(size) = attrs.size {
            // pangocairo's default DPI is 96 and we don't want to change it, so
            // apply a scaling factor here
            const FACTOR: f32 = 72.0 / 96.0;
            font_desc.set_size((size * (pango::SCALE as f32 * FACTOR)) as i32);
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

impl TextLayout {
    pub(super) fn lock_layout(&self) -> &Layout {
        // TODO: actually lock
        &self.pango_layout.inner
    }
}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        let font_map = pangocairo::FontMap::get_default().expect("failed to get a Pango font map");

        let ctx = font_map
            .create_context()
            .expect("failed to create pango context");

        // Create `Layout`
        let layout = pango::Layout::new(&ctx);

        layout.set_font_description(Some(&style.pango_font_desc.inner));

        if let Some(x) = width {
            layout.set_width(
                (x * pango::SCALE as f32)
                    .fmin(i32::max_value() as f32)
                    .fmax(0.0) as i32,
            );
        }

        layout.set_text(text);

        // TODO: `decor`

        Self {
            pango_layout: ImmutableLayout { inner: layout },
        }
    }

    // TODO: see if `update_layout` messes up the extents

    fn visual_bounds(&self) -> Box2<f32> {
        let (ink_rect, _logical_rect) = self.lock_layout().get_extents();
        pango_rect_to_box2_f32(ink_rect)
    }

    fn layout_bounds(&self) -> Box2<f32> {
        let (_ink_rect, logical_rect) = self.lock_layout().get_extents();
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
