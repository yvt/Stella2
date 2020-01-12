use cggeom::Box2;
use directwrite::{enums::FontWeight, factory::Factory};
use std::fmt;

use crate::iface;

lazy_static::lazy_static! {
    static ref G: Global = Global::new();
}

struct Global {
    dwrite: Factory,
}

impl Global {
    fn new() -> Self {
        let dwrite = Factory::new().unwrap();

        Self { dwrite }
    }
}

#[derive(Debug, Clone)]
pub struct CharStyle {
    size: f32,
    weight: FontWeight,
    color: Option<iface::RGBAF32>,
    decor: iface::TextDecorFlags,
}

pub type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

impl iface::CharStyle for CharStyle {
    fn new(attrs: CharStyleAttrs) -> Self {
        let mut cs = if let Some(template) = attrs.template {
            template
        } else {
            use crate::iface::SysFontType;
            let (rel_size, weight) = match attrs.sys.unwrap_or(SysFontType::Normal) {
                SysFontType::Normal => (1.0, FontWeight::Normal),
                SysFontType::Emph => (1.0, FontWeight::Bold),
                SysFontType::Small => (0.8, FontWeight::Normal),
                SysFontType::SmallEmph => (0.8, FontWeight::Bold),
                SysFontType::User => (1.0, FontWeight::Normal), // TODO
                SysFontType::UserMonospace => (1.0, FontWeight::Normal), // TODO
            };
            CharStyle {
                size: 12.0 * rel_size,
                weight,
                color: None,
                decor: iface::TextDecorFlags::empty(),
            }
        };

        if let Some(size) = attrs.size {
            cs.size = size;
        }

        if let Some(decor) = attrs.decor {
            cs.decor = decor;
        }

        if let Some(color) = attrs.color {
            cs.color = color;
        }

        cs
    }

    fn size(&self) -> f32 {
        self.size
    }
}

impl CharStyle {
    fn to_dwrite_format(&self) -> directwrite::TextFormat {
        directwrite::TextFormat::create(&G.dwrite)
            .with_family("Segoe UI") // TODO
            .with_size(self.size)
            .with_weight(self.weight)
            .build()
            .unwrap()
    }
}

pub struct TextLayout {
    dwrite_layout: directwrite::TextLayout,
    color: Option<iface::RGBAF32>,
}

impl fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextLayout")
            .field("dwrite_layout", &unsafe { self.dwrite_layout.get_raw() })
            .field("color", &self.color)
            .finish()
    }
}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        let dwrite_layout = directwrite::TextLayout::create(&G.dwrite)
            .with_text(text)
            .with_width(width.unwrap_or(std::f32::INFINITY))
            .with_height(0.0)
            .with_font(&style.to_dwrite_format())
            .build()
            .unwrap();

        if style.decor.contains(iface::TextDecorFlags::UNDERLINE) {
            dwrite_layout.set_underline(true, ..).unwrap();
        }
        if style.decor.contains(iface::TextDecorFlags::STRIKETHROUGH) {
            dwrite_layout.set_strikethrough(true, ..).unwrap();
        }
        // TODO: TextDecorFlags::OVERLINE

        Self {
            dwrite_layout,
            color: style.color,
        }
    }

    fn visual_bounds(&self) -> Box2<f32> {
        let met = self.dwrite_layout.get_metrics();
        let ohmet = self.dwrite_layout.get_overhang_metrics();

        let layout_width = met.layout_width();
        let right = if layout_width.is_finite() {
            layout_width + ohmet.right()
        } else {
            // If the layout width is unspecified, `ohmet.right()` is useless
            met.width()
        };

        cggeom::box2! {
            min: [-ohmet.left(), -ohmet.top()],
            max: [right, ohmet.bottom()],
        }
    }

    fn layout_bounds(&self) -> Box2<f32> {
        let met = self.dwrite_layout.get_metrics();

        cggeom::box2! {
            min: [0.0, 0.0],
            max: [met.left() + met.width(), met.top() + met.height()],
        }
    }
}
