use cgmath::Point2;
use core_foundation::{
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics::{
    geometry::{CGPoint, CGRect, CGSize},
    path::CGPath,
};
use core_text::{
    font as ct_font,
    font::{CTFont, CTFontRef},
    frame::CTFrame,
    framesetter::{CTFramesetter, CTFramesetterRef},
    string_attributes,
};
use std::os::raw::c_void;

use super::super::{
    iface,
    iface::{Canvas, RGBAF32},
};
use super::BitmapBuilder;

#[derive(Debug, Clone)]
pub struct CharStyle {
    font: CTFont,
}

unsafe impl Send for CharStyle {}
unsafe impl Sync for CharStyle {}

impl iface::CharStyle for CharStyle {
    fn new(mut attrs: iface::CharStyleAttrs<Self>) -> Self {
        let font = attrs.sys.map(|ty| {
            let ty = match ty {
                iface::SysFontType::Normal => ct_font::kCTFontSystemFontType,
                iface::SysFontType::Emph => ct_font::kCTFontEmphasizedSystemFontType,
                iface::SysFontType::Small => ct_font::kCTFontSmallSystemFontType,
                iface::SysFontType::SmallEmph => ct_font::kCTFontSmallEmphasizedSystemFontType,
                iface::SysFontType::User => ct_font::kCTFontUserFontType,
                iface::SysFontType::UserMonospace => ct_font::kCTFontUserFixedPitchFontType,
            };

            ctfont_new_ui(ty, attrs.size.take().unwrap_or(0.0) as f64, None)
        });

        let font = font.or_else(|| {
            attrs.template.take().map(|old_style| {
                if let Some(size) = attrs.size.take() {
                    old_style.font.clone_with_font_size(size as f64)
                } else {
                    old_style.font.clone()
                }
            })
        });

        let font = font.unwrap_or_else(|| {
            ctfont_new_ui(
                ct_font::kCTFontSystemFontType,
                attrs.size.take().unwrap_or(0.0) as f64,
                None,
            )
        });

        // TODO: other attributes: `decor`, `color`

        Self { font }
    }

    fn size(&self) -> f32 {
        self.font.pt_size() as f32
    }
}

#[derive(Debug)]
pub struct TextLayout {
    frame: CTFrame,
}

unsafe impl Send for TextLayout {}
unsafe impl Sync for TextLayout {}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        let mut attr_str = CFMutableAttributedString::new();
        attr_str.replace_str(&text.into(), CFRange::init(0, 0));

        let text_range = CFRange::init(0, text.len() as i64);
        attr_str.set_attribute(
            text_range,
            unsafe { string_attributes::kCTFontAttributeName },
            style.font.clone(),
        );

        attr_str.set_attribute::<CFNumber>(
            text_range,
            unsafe { string_attributes::kCTForegroundColorFromContextAttributeName },
            1i32.into(),
        );

        // TODO: other attributes

        let framesetter = CTFramesetter::new_with_attributed_string(attr_str.as_concrete_TypeRef());

        let frame_size_constraint = CGSize::new(
            width.map(|x| x as f64).unwrap_or(std::f64::MAX),
            std::f64::MAX,
        );
        let (frame_size, _) =
            ctframesetter_suggest_frame_size(&framesetter, text_range, frame_size_constraint);

        let frame_path = CGPath::from_rect(
            CGRect::new(&CGPoint::new(0.0, -frame_size.height), &frame_size),
            None,
        );

        let frame = framesetter.create_frame(text_range, &frame_path);

        Self { frame }
    }
}

impl iface::CanvasText<TextLayout> for BitmapBuilder {
    fn draw_text(&mut self, layout: &TextLayout, origin: Point2<f32>, color: RGBAF32) {
        self.cg_context.save();
        self.cg_context.translate(origin.x as f64, origin.y as f64);
        self.cg_context.scale(1.0, -1.0);
        self.set_fill_rgb(color);
        layout.frame.draw(&self.cg_context);
        self.cg_context.restore();
    }
}

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    fn CTFontCreateUIFontForLanguage(
        ui_type: ct_font::CTFontUIFontType,
        size: f64,
        language: CFStringRef,
    ) -> CTFontRef;

    fn CTFramesetterSuggestFrameSizeWithConstraints(
        framesetter: CTFramesetterRef,
        string_range: CFRange,
        frame_attributes: *const c_void,
        constraints: CGSize,
        fit_range: *mut CFRange,
    ) -> CGSize;
}

fn ctfont_new_ui(ty: ct_font::CTFontUIFontType, size: f64, language: Option<&str>) -> CTFont {
    unsafe {
        let name: Option<CFString> = language.map(|s| s.into());
        let font_ref = CTFontCreateUIFontForLanguage(
            ty,
            size,
            name.map(|n| n.as_concrete_TypeRef())
                .unwrap_or(std::ptr::null_mut()),
        );

        CTFont::wrap_under_create_rule(font_ref)
    }
}

fn ctframesetter_suggest_frame_size(
    this: &CTFramesetter,
    string_range: CFRange,
    constraints: CGSize,
) -> (CGSize, CFRange) {
    unsafe {
        let mut fit_range: CFRange = std::mem::uninitialized();
        let size = CTFramesetterSuggestFrameSizeWithConstraints(
            this.as_concrete_TypeRef(),
            string_range,
            std::ptr::null(),
            constraints,
            &mut fit_range,
        );
        (size, fit_range)
    }
}
