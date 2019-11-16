use alt_fp::FloatOrd;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{vec2, Point2};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics::{
    base::CGFloat,
    color_space::CGColorSpace,
    context::{CGContext, CGContextRef},
    geometry::{CGPoint, CGRect, CGSize},
    image::CGImageAlphaInfo,
    path::CGPath,
};
use core_text::{
    font as ct_font,
    font::{CTFont, CTFontRef},
    frame::{CTFrame, CTFrameRef},
    framesetter::{CTFramesetter, CTFramesetterRef},
    line::{CTLine, CTLineRef},
    string_attributes,
};
use lazy_static::lazy_static;
use std::{
    f32::{INFINITY, NEG_INFINITY},
    mem::MaybeUninit,
    os::raw::c_void,
};

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
                    old_style.font
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
    height: f32,
}

unsafe impl Send for TextLayout {}
unsafe impl Sync for TextLayout {}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        let mut attr_str = CFMutableAttributedString::new();
        attr_str.replace_str(&text.into(), CFRange::init(0, 0));

        let text_range = CFRange::init(0, attr_str.char_len());
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

        Self {
            frame,
            height: frame_size.height as f32,
        }
    }

    fn visual_bounds(&self) -> Box2<f32> {
        let (lines, origins) = self.get_lines_and_line_origins();

        struct ContextCell(CGContext);

        // `CTLineGetImageBounds` doesn't mutate `CGContext`, I suppose.
        unsafe impl Send for ContextCell {}
        unsafe impl Sync for ContextCell {}

        // From the documentation of `CTLineGetImageBounds`
        // (https://developer.apple.com/documentation/coretext/ctline):
        //
        // > The context for which the image bounds are calculated. This is
        // > required because the context could have settings in it that woul
        // > cause changes in the image bounds.
        lazy_static! {
            static ref ATTR_CONTEXT: ContextCell = ContextCell({
                CGContext::create_bitmap_context(
                    None,
                    1,
                    1,
                    8,
                    0,
                    &CGColorSpace::create_device_rgb(),
                    CGImageAlphaInfo::CGImageAlphaPremultipliedLast as u32,
                )
            });
        }

        let mut bounds = box2! {
            min: [INFINITY, INFINITY],
            max: [NEG_INFINITY, NEG_INFINITY],
        };

        for (line, line_origin) in lines.iter().zip(origins.iter()) {
            let image_bounds = ctline_get_image_bounds(&line, &ATTR_CONTEXT.0);

            // The line origin points returned by the API are apparently
            // relative to the path used to create the `CTFrame`, so we need
            // to use `self.height` here to figure out their absolute
            // coordeinates
            let line_origin = vec2(line_origin.x as f32, self.height - line_origin.y as f32);

            bounds.union_assign(
                &box2! {
                    bottom_left: [
                        image_bounds.origin.x as f32,
                        -(image_bounds.origin.y as f32),
                    ],
                    size: [
                        image_bounds.size.width as f32,
                        image_bounds.size.height as f32,
                    ],
                }
                .translate(line_origin),
            );
        }

        bounds
    }

    fn layout_bounds(&self) -> Box2<f32> {
        let (lines, origins) = self.get_lines_and_line_origins();

        let mut bounds = box2! { min: [INFINITY, 0.0], max: [NEG_INFINITY, 0.0] };

        for (line, line_origin) in lines.iter().zip(origins.iter()) {
            let typo_bounds = ctline_get_typographic_bounds(&line);

            // See the comment in `visual_bounds`.
            let line_origin = vec2(line_origin.x as f32, self.height - line_origin.y as f32);

            bounds.min.x = bounds.min.x.fmin(line_origin.x);
            bounds.max.x = bounds.max.x.fmax(line_origin.x + typo_bounds.width as f32);

            // Not sure how to calculate the updated bottom position...
            // I couldn't find the defiition of these metrics values anywhere
            // in Core Text's documentation. Might wanna double-check as soon as
            // I start working on other backends.
            bounds.max.y = line_origin.y + (typo_bounds.leading + typo_bounds.descent) as f32;
        }

        bounds
    }
}

impl TextLayout {
    fn get_lines_and_line_origins(&self) -> (CFArray<CTLine>, Vec<CGPoint>) {
        let lines = ctframe_get_lines(&self.frame);
        let mut origins = vec![CGPoint::new(0.0, 0.0); lines.len() as usize];
        ctframe_get_line_origins(&self.frame, 0, &mut origins[..]);
        (lines, origins)
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

    fn CTFrameGetLines(frame: CTFrameRef) -> CFArrayRef;

    fn CTFrameGetLineOrigins(frame: CTFrameRef, range: CFRange, origins: *mut CGPoint);

    fn CTLineGetTypographicBounds(
        line: CTLineRef,
        ascent: *mut CGFloat,
        descent: *mut CGFloat,
        leading: *mut CGFloat,
    ) -> CGFloat;

    fn CTLineGetImageBounds(line: CTLineRef, context: *const u8) -> CGRect;
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
        let mut fit_range = MaybeUninit::<CFRange>::uninit();
        let size = CTFramesetterSuggestFrameSizeWithConstraints(
            this.as_concrete_TypeRef(),
            string_range,
            std::ptr::null(),
            constraints,
            fit_range.as_mut_ptr(),
        );
        (size, fit_range.assume_init())
    }
}

fn ctframe_get_lines(this: &CTFrame) -> CFArray<CTLine> {
    unsafe {
        let array_ref = CTFrameGetLines(this.as_concrete_TypeRef());
        CFArray::wrap_under_get_rule(array_ref)
    }
}

fn ctframe_get_line_origins(this: &CTFrame, start_line: isize, out_origins: &mut [CGPoint]) {
    use std::convert::TryInto;
    assert!(
        (out_origins.len() as u64) <= <i64>::max_value() as u64,
        "integer overflow"
    );
    let range = CFRange::init(
        start_line,
        start_line
            .checked_add(out_origins.len().try_into().expect("integer overflow"))
            .expect("integer overflow"),
    );
    unsafe {
        CTFrameGetLineOrigins(this.as_concrete_TypeRef(), range, out_origins.as_mut_ptr());
    }
}

#[derive(Default, Debug, Copy, Clone)]
struct TypographicBounds {
    width: CGFloat,
    ascent: CGFloat,
    descent: CGFloat,
    leading: CGFloat,
}

fn ctline_get_typographic_bounds(this: &CTLine) -> TypographicBounds {
    let mut bounds = TypographicBounds::default();
    unsafe {
        bounds.width = CTLineGetTypographicBounds(
            this.as_concrete_TypeRef(),
            &mut bounds.ascent,
            &mut bounds.descent,
            &mut bounds.leading,
        );
    }
    bounds
}

fn ctline_get_image_bounds(this: &CTLine, context: &CGContextRef) -> CGRect {
    unsafe { CTLineGetImageBounds(this.as_concrete_TypeRef(), context as *const _ as *const u8) }
}
