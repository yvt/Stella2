use alt_fp::FloatOrd;
use atom2::SetOnceAtom;
use cggeom::{box2, prelude::*, Box2};
use cgmath::{vec2, Point2};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    attributed_string::CFMutableAttributedString,
    base::{CFIndex, CFRange, TCFType},
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
    run::{CTRun, CTRunRef},
    string_attributes,
};
use lazy_static::lazy_static;
use std::{
    f32::{INFINITY, NEG_INFINITY},
    mem::MaybeUninit,
    ops::Range,
    os::raw::c_void,
    slice,
};
use utf16count::{find_utf16_pos_in_utf8_str, utf16_len_of_utf8_str};

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
    text: String,
    line_boundaries: SetOnceAtom<Box<Box<[usize]>>>,
    line_origins: Box<[CGPoint]>,
}

unsafe impl Send for TextLayout {}
unsafe impl Sync for TextLayout {}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        let mut attr_str = CFMutableAttributedString::new();

        // Make sure the last line is not omitted
        let last_byte = text.as_bytes().last();
        if matches!(last_byte, None | Some(b'\n') | Some(b'\r')) {
            attr_str.replace_str(&" ".into(), CFRange::init(0, 0));
        }

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

        let lines = ctframe_get_lines(&frame);
        let mut line_origins = vec![CGPoint::new(0.0, 0.0); lines.len() as usize];
        ctframe_get_line_origins(&frame, 0, &mut line_origins[..]);

        debug_assert!(lines.len() > 0, "The `CTFrame` has no lines");

        Self {
            frame,
            height: frame_size.height as f32,
            text: text.to_owned(),
            line_boundaries: SetOnceAtom::empty(),
            line_origins: line_origins.into(),
        }
    }

    fn visual_bounds(&self) -> Box2<f32> {
        let lines = ctframe_get_lines(&self.frame);
        let origins = &self.line_origins[..];

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
        let lines = ctframe_get_lines(&self.frame);
        let origins = &self.line_origins[..];

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

    fn cursor_index_from_point(&self, point: Point2<f32>) -> usize {
        let lines = ctframe_get_lines(&self.frame);
        let origins = &self.line_origins;

        // See the comment in `visual_bounds`.
        let layout_y = (self.height - point.y) as f64;

        let get_bottom = |line_i: usize| {
            let typo_bounds = ctline_get_typographic_bounds(&lines.get(line_i as _).unwrap());
            let line_origin = origins[line_i];
            line_origin.y - (typo_bounds.leading + typo_bounds.descent)
        };

        // Find the line containing `point`
        let line = {
            let mut base = 0;
            let mut size = self.num_lines();

            while size > 1 {
                let half = size / 2;
                let mid = base + half;
                base = if layout_y < get_bottom(mid - 1) {
                    mid
                } else {
                    base
                };
                size -= half;
            }

            base
        };

        let ctline = lines.get(line as _).unwrap();

        let line_start = self.line_index_range(line).start;
        let line_start_u16 = ctline_get_string_range(&ctline).location;

        // Find the character position in the line
        let i_u16 =
            ctline_get_string_index_for_position(&ctline, CGPoint::new(point.x as f64, 0.0));

        line_start
            + find_utf16_pos_in_utf8_str(
                (i_u16 - line_start_u16) as usize,
                &self.text.as_bytes()[line_start..],
            )
            .utf8_cursor
    }

    fn cursor_pos(&self, i: usize) -> [iface::Beam; 2] {
        let lines = ctframe_get_lines(&self.frame);

        let line = self.line_from_index(i);
        let ctline = lines.get(line as isize).unwrap();

        let line_start = self.line_index_range(line).start;
        let line_start_u16 = ctline_get_string_range(&ctline).location;
        let line_vert_bounds = self.line_vertical_bounds(line);

        let rel_i_u16 = utf16_len_of_utf8_str(&self.text.as_bytes()[line_start..i]);

        let offsets =
            ctline_get_offset_for_string_index(&ctline, rel_i_u16 as CFIndex + line_start_u16);

        // The documentation of `CTLineGetOffsetForStringIndex` says:
        //
        // <https://developer.apple.com/documentation/coretext/1509629-ctlinegetoffsetforstringindex>:
        // > the returned primary offset corresponds to the portion of the
        // > caret that represents the visual insertion location for a character
        // > whose direction matches the line's writing direction.
        //
        // Actually, it appears that there are some cases where the primary
        // offset and the secondary offset are swapped.

        use array::Array2;
        offsets.map(|x| iface::Beam {
            x: x as f32,
            top: line_vert_bounds.start,
            bottom: line_vert_bounds.end,
        })
    }

    fn num_lines(&self) -> usize {
        self.line_origins.len()
    }

    fn line_index_range(&self, i: usize) -> Range<usize> {
        let line_boundaries = self
            .line_boundaries
            .get_or_racy_insert_with(|| {
                let lines = ctframe_get_lines(&self.frame);
                let text = &self.text[..];
                let mut cur_u16 = 0;
                let mut cur_u8 = 0;
                let lines: Box<[usize]> = lines
                    .iter()
                    .map(|line| {
                        let range = ctline_get_string_range(&line);
                        let adv_u16 = range.location as usize - cur_u16;
                        let adv_u8 =
                            find_utf16_pos_in_utf8_str(adv_u16, &text.as_bytes()[cur_u8..])
                                .utf8_cursor;
                        cur_u16 += adv_u16;
                        cur_u8 += adv_u8;
                        cur_u8
                    })
                    .chain(std::iter::once(text.len()))
                    .collect::<Vec<_>>()
                    // Converting to `Box<[usize]>` involves no reallocation
                    // because this iterator is `ExactSizeIterator`
                    .into();
                Box::new(lines)
            })
            .0;

        debug_assert_eq!(line_boundaries.len(), self.num_lines() + 1);

        let the_boundaries = &line_boundaries[i..][..2];
        the_boundaries[0]..the_boundaries[1]
    }

    fn line_vertical_bounds(&self, i: usize) -> Range<f32> {
        let lines = ctframe_get_lines(&self.frame);
        let origins = &self.line_origins;

        let get_bottom = |line_i: usize| {
            let typo_bounds = ctline_get_typographic_bounds(&lines.get(line_i as _).unwrap());

            let line_origin = origins[line_i];

            // See the comment in `visual_bounds`.
            let line_origin_y = self.height - line_origin.y as f32;

            line_origin_y + (typo_bounds.leading + typo_bounds.descent) as f32
        };

        let top = if i == 0 { 0.0 } else { get_bottom(i - 1) };
        let bottom = get_bottom(i);

        top..bottom
    }

    fn line_baseline(&self, i: usize) -> f32 {
        let lines = ctframe_get_lines(&self.frame);
        let typo_bounds = ctline_get_typographic_bounds(&lines.get(i as _).unwrap());
        self.height - self.line_origins[i].y as f32 + typo_bounds.ascent as f32 * 0.1
    }

    fn run_metrics_of_range(&self, range: Range<usize>) -> Vec<iface::RunMetrics> {
        let lines = ctframe_get_lines(&self.frame);

        debug_assert_ne!(range.start, range.end, "The range mustn't be empty");

        let line = self.line_from_index(range.start);
        debug_assert_eq!(
            self.line_from_index(range.end - 1),
            line,
            "The range mustn't span across lines"
        );
        let ctline = lines.get(line as _).unwrap();

        let line_range = self.line_index_range(line);
        let line_range_u16 = ctline_get_string_range(&ctline);
        let line_start = line_range.start;

        let text = self.text.as_bytes();

        let start_u16 = utf16_len_of_utf8_str(&text[line_start..range.start])
            + line_range_u16.location as usize;
        let end_u16 = start_u16 + utf16_len_of_utf8_str(&text[range.clone()]);

        // `CTRun`s for the current line sorted by a visual order (left to right)
        let glyph_runs: CFArray<CTRun> = ctline.glyph_runs();

        // Collect the endpoints. Sort them, which lets us compute their
        // mapping to UTF-8 offsets in `O(line_len_8)`.
        //
        // The ranges represented by `glyph_runs` are a partition of
        // `line_range_u16`. This means that if we add
        // `ctrun_get_string_range(run).end`, we are also adding
        // `ctrun_get_string_range(run).start` for the subsequent `run2` unless
        // `run2` doesn't exist.
        //
        //                     v run.pos+run.len     v run2.pos+run2.len
        //      ┌──────────────┬─────────────────────┐
        //      | run          | run2                |
        //      └──────────────┴─────────────────────┘
        //      ^ run.pos      ^ run2.pos
        //
        // However, `ctrun_get_string_range(run2).start` isn't added for the
        // earliest run because there's no corresponding `run`. This case is
        // covered by `once((min_u16, 0))`.
        let mut eps: Vec<(usize, usize)> = std::iter::once((line_range_u16.location as usize, 0))
            .chain(glyph_runs.iter().map(|run| {
                let range = ctrun_get_string_range(&run);
                ((range.location + range.length) as usize, 0)
            }))
            .filter(|&(pos_u16, _)| pos_u16 > start_u16 && pos_u16 < end_u16)
            .collect();
        eps.push((start_u16, 0));
        eps.push((end_u16, 0));
        minisort::minisort_by_key(&mut eps, |&(pos_u16, _)| pos_u16);

        // Fill the second field of each tuple of `eps`
        eps.iter_mut().fold(
            (start_u16, range.start),
            |(last_u16, last_u8), (pos_u16, pos_u8)| {
                *pos_u8 = find_utf16_pos_in_utf8_str(*pos_u16 - last_u16, &text[last_u8..])
                    .utf8_cursor
                    + last_u8;

                (*pos_u16, *pos_u8)
            },
        );
        log::trace!("  eps = {:?}", eps);

        // Define a closure that converts text offsets using `eps`.
        let utf16_to_utf8 = |pos_u16| {
            let i = eps
                .binary_search_by_key(&pos_u16, |&(ep_u16, _)| ep_u16)
                // `pos_u16` must be included in `eps`
                .unwrap();
            let (_, ep_u8) = eps[i];
            ep_u8
        };

        let mut out_run_metrics = Vec::new();

        for run in glyph_runs.iter() {
            let run_status = ctrun_get_status(&run);
            let glyph_count = run.glyph_count();

            // TODO: Handle `kCTRunStatusHasNonIdentityMatrix`
            if (run_status & kCTRunStatusHasNonIdentityMatrix) != 0 {
                log::warn!("run_metrics_of_range: todo! `kCTRunStatusHasNonIdentityMatrix`");
            }

            let glyph_str_range = ctrun_get_string_range(&run);
            let glyph_str_start_u16 = glyph_str_range.location as usize;
            let glyph_str_end_u16 = (glyph_str_range.location + glyph_str_range.length) as usize;

            if glyph_str_start_u16 >= end_u16 || glyph_str_end_u16 <= start_u16 {
                // The run doesn't overlap with `range`
                continue;
            }

            // Convert `CTRunStatus` to `RunFlags`
            let mut flags = iface::RunFlags::empty();
            let is_run_rtl = (run_status & kCTRunStatusRightToLeft) != 0;
            if is_run_rtl {
                flags |= iface::RunFlags::RIGHT_TO_LEFT;
            }

            let run_left = ctrun_get_positions_one(&run, 0).x;
            let run_right = ctrun_get_positions_one(&run, glyph_count - 1).x
                + ctrun_get_advances_one(&run, glyph_count - 1).width;

            if glyph_str_start_u16 >= start_u16 && glyph_str_end_u16 <= end_u16 {
                // The run is completely contained by `range`
                let glyph_str_start_u8 = utf16_to_utf8(glyph_str_start_u16);
                let glyph_str_end_u8 = utf16_to_utf8(glyph_str_end_u16);

                out_run_metrics.push(iface::RunMetrics {
                    flags,
                    index: glyph_str_start_u8..glyph_str_end_u8,
                    bounds: run_left as f32..run_right as f32,
                });
                continue;
            }

            // A note for complexity analysis:
            // The rest of this block will run only up to twice during
            // a single call to `run_metrics_of_range`
            if (run_status & kCTRunStatusNonMonotonic) != 0 {
                // I don't know what could cause the monotonicity. For now we
                // assume it's not important for `run_metrics_of_range`.
                log::trace!(
                    "run_metrics_of_range: Ignoring `kCTRunStatusNonMonotonic`. text = {:?}",
                    self.text
                );
            }

            // Clip the current run by the range `start_u16..end_u16`.
            // Use `CTLineGetOffsetForStringIndex` to derive the visual position
            // for the endpoints that fall within the run. We can ignore the
            // secondary offset returned by `CTLineGetOffsetForStringIndex`
            // because such endpoints are not on a writing direction boundary.
            let (out_run_visual_start, out_start_u16) = if glyph_str_start_u16 >= start_u16 {
                (
                    if is_run_rtl { run_right } else { run_left },
                    glyph_str_start_u16,
                )
            } else {
                (
                    ctline_get_primary_offset_for_string_index(&ctline, start_u16 as CFIndex),
                    start_u16,
                )
            };

            let (out_run_visual_end, out_end_u16) = if glyph_str_end_u16 <= end_u16 {
                (
                    if is_run_rtl { run_left } else { run_right },
                    glyph_str_end_u16,
                )
            } else {
                (
                    ctline_get_primary_offset_for_string_index(&ctline, end_u16 as CFIndex),
                    end_u16,
                )
            };

            let glyph_str_start_u8 = utf16_to_utf8(out_start_u16);
            let glyph_str_end_u8 = utf16_to_utf8(out_end_u16);

            let mut out_run_visual_start = out_run_visual_start as f32;
            let mut out_run_visual_end = out_run_visual_end as f32;

            if is_run_rtl {
                std::mem::swap(&mut out_run_visual_start, &mut out_run_visual_end);
            }

            out_run_metrics.push(iface::RunMetrics {
                flags,
                index: glyph_str_start_u8..glyph_str_end_u8,
                bounds: out_run_visual_start..out_run_visual_end,
            });
        }

        out_run_metrics
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

    fn CTLineGetStringRange(line: CTLineRef) -> CFRange;

    fn CTLineGetOffsetForStringIndex(
        line: CTLineRef,
        charIndex: CFIndex,
        secondaryOffset: *mut CGFloat,
    ) -> CGFloat;

    fn CTLineGetStringIndexForPosition(line: CTLineRef, position: CGPoint) -> CFIndex;

    fn CTRunGetStringRange(run: CTRunRef) -> CFRange;

    fn CTRunGetStatus(run: CTRunRef) -> CTRunStatus;

    fn CTRunGetAdvances(run: CTRunRef, range: CFRange, buffer: *mut CGSize);

    fn CTRunGetPositions(run: CTRunRef, range: CFRange, buffer: *mut CGPoint);

    fn CTRunGetStringIndicesPtr(run: CTRunRef) -> *const CFIndex;

    fn CTRunGetStringIndices(run: CTRunRef, range: CFRange, buffer: *mut CFIndex);
}

type CTRunStatus = u32;

#[allow(non_upper_case_globals)]
const kCTRunStatusRightToLeft: CTRunStatus = 1;
#[allow(non_upper_case_globals)]
const kCTRunStatusNonMonotonic: CTRunStatus = 1 << 1;
#[allow(non_upper_case_globals)]
const kCTRunStatusHasNonIdentityMatrix: CTRunStatus = 1 << 2;

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
        out_origins.len().try_into().expect("integer overflow"),
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

fn ctline_get_string_range(this: &CTLine) -> CFRange {
    unsafe { CTLineGetStringRange(this.as_concrete_TypeRef()) }
}

fn ctline_get_offset_for_string_index(this: &CTLine, char_index: CFIndex) -> [CGFloat; 2] {
    unsafe {
        let mut sec = MaybeUninit::uninit();
        let pri =
            CTLineGetOffsetForStringIndex(this.as_concrete_TypeRef(), char_index, sec.as_mut_ptr());
        [pri, sec.assume_init()]
    }
}

fn ctline_get_primary_offset_for_string_index(this: &CTLine, char_index: CFIndex) -> CGFloat {
    unsafe {
        CTLineGetOffsetForStringIndex(this.as_concrete_TypeRef(), char_index, std::ptr::null_mut())
    }
}

fn ctline_get_string_index_for_position(this: &CTLine, position: CGPoint) -> CFIndex {
    unsafe { CTLineGetStringIndexForPosition(this.as_concrete_TypeRef(), position) }
}

fn ctrun_get_string_range(this: &CTRun) -> CFRange {
    unsafe { CTRunGetStringRange(this.as_concrete_TypeRef()) }
}

fn ctrun_get_status(this: &CTRun) -> CTRunStatus {
    unsafe { CTRunGetStatus(this.as_concrete_TypeRef()) }
}

#[allow(dead_code)]
fn ctrun_get_advances(this: &CTRun, start: isize, out_advances: &mut [MaybeUninit<CGSize>]) {
    if out_advances.len() == 0 {
        return;
    }

    use std::convert::TryInto;
    assert!(
        (out_advances.len() as u64) <= <i64>::max_value() as u64,
        "integer overflow"
    );
    let range = CFRange::init(
        start,
        out_advances.len().try_into().expect("integer overflow"),
    );
    unsafe {
        CTRunGetAdvances(
            this.as_concrete_TypeRef(),
            range,
            out_advances.as_mut_ptr() as _,
        );
    }
}

fn ctrun_get_advances_one(this: &CTRun, i: CFIndex) -> CGSize {
    unsafe {
        let mut out = MaybeUninit::uninit();
        CTRunGetAdvances(
            this.as_concrete_TypeRef(),
            CFRange::init(i, 1),
            out.as_mut_ptr(),
        );
        out.assume_init()
    }
}

#[allow(dead_code)]
fn ctrun_get_advances_vec(this: &CTRun, range: CFRange, out_advances: &mut Vec<CGSize>) {
    use std::convert::TryInto;
    let len: usize = range.length.try_into().expect("integer overflow");
    unsafe {
        out_advances.clear();
        out_advances.reserve(len);
        ctrun_get_advances(
            this,
            range.location,
            slice::from_raw_parts_mut(out_advances.as_mut_ptr() as _, len),
        );
        out_advances.set_len(len);
    }
}

fn ctrun_get_positions_one(this: &CTRun, i: CFIndex) -> CGPoint {
    unsafe {
        let mut out = MaybeUninit::uninit();
        CTRunGetPositions(
            this.as_concrete_TypeRef(),
            CFRange::init(i, 1),
            out.as_mut_ptr(),
        );
        out.assume_init()
    }
}

#[allow(dead_code)]
fn ctrun_get_all_string_indices<'a>(
    this: &'a CTRun,
    buffer: &'a mut Vec<CFIndex>,
) -> &'a [CFIndex] {
    use std::convert::TryInto;
    let count = this.glyph_count();
    let count_usize: usize = count.try_into().expect("integer overflow");

    // Try `CTRunGetStringIndicesPtr` first because it doesn't involve copying
    unsafe {
        let ptr = CTRunGetStringIndicesPtr(this.as_concrete_TypeRef());
        if !ptr.is_null() {
            return slice::from_raw_parts(ptr, count_usize);
        }
    }

    unsafe {
        buffer.clear();
        buffer.reserve(count_usize);
        CTRunGetStringIndices(
            this.as_concrete_TypeRef(),
            CFRange::init(0, count),
            buffer.as_mut_ptr(),
        );
        buffer.set_len(count_usize);
    }

    &buffer[..]
}
