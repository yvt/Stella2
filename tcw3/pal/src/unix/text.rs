use alt_fp::FloatOrd;
use cggeom::{box2, Box2};
use cgmath::Point2;
use flags_macro::flags;
use pango::{FontDescription, FontMapExt, Layout, LayoutLine};
use rgb::RGBA16;
use std::{
    convert::TryInto, ffi::CStr, mem::MaybeUninit, ops::Range, os::raw::c_uint, sync::Mutex,
};
use unicount::{num_scalars_in_utf8_str, str_next, str_prev};

use super::super::iface;

type CharStyleAttrs = iface::CharStyleAttrs<CharStyle>;

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
                    font_desc.set_family_static_safe(CStr::from_bytes_with_nul(b"Sans\0").unwrap());
                }
                SysFontType::UserMonospace => {
                    font_desc
                        .set_family_static_safe(CStr::from_bytes_with_nul(b"Monospace\0").unwrap());
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
    text_len: usize,
    line_metrics: Vec<LineMetrics>,
}

#[derive(Debug)]
struct LineMetrics {
    baseline: f32,
    logical_extents: Range<f32>,
    start_index: i32,
}

#[derive(Debug)]
struct ImmutableLayout {
    inner: Mutex<Layout>,
}

// I think `Layout`'s thread unsafety comes from mutability
// TODO: Fact-check this
unsafe impl Send for ImmutableLayout {}
unsafe impl Sync for ImmutableLayout {}

impl TextLayout {
    pub(super) fn lock_layout(&self) -> impl std::ops::Deref<Target = Layout> + '_ {
        self.pango_layout.inner.lock().unwrap()
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

        let num_lines = layout.get_line_count() as usize;
        log::trace!("The text {:?} generated {:?} line(s)", text, num_lines);

        let line_metrics: Vec<LineMetrics> = {
            let mut iter = layout.get_iter().unwrap();
            (0..num_lines)
                .map(|_| {
                    let baseline = iter.get_baseline();

                    let (_, logical_extents) = iter.get_line_extents();
                    let logical_top = pango_coord_to_f32(logical_extents.y);
                    let logical_bottom =
                        pango_coord_to_f32(logical_extents.y + logical_extents.height);

                    let start_index = iter.get_line_readonly().unwrap().start_index();

                    iter.next_line();

                    LineMetrics {
                        baseline: pango_coord_to_f32(baseline),
                        logical_extents: logical_top..logical_bottom,
                        start_index,
                    }
                })
                .collect()
        };
        log::trace!("line_metrics = {:?}", line_metrics);

        Self {
            pango_layout: ImmutableLayout {
                inner: Mutex::new(layout),
            },
            text_len: text.len(),
            line_metrics,
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

    fn cursor_index_from_point(&self, point: Point2<f32>) -> usize {
        let [x, y] = point_to_pango_xy(point);

        let layout = self.lock_layout();
        let (_, index, trailing) = layout.xy_to_index(x, y);

        // We want the closest edge (rather than the starting index of the
        // grapheme containing the point), so add `trailing` to get the final
        // index. `trailing` is measured in "the number of characters", not
        // UTF-8 bytes.
        //
        // First, we get the source string. These methods are actually supposed
        // to be `unsafe` (see the discussion in `run_metrics_of_range`).
        let text = layout.get_text().unwrap();
        let text = text.as_str();

        // Advance the cursor by `trailing` scalars
        let mut i = index as usize;
        for _ in 0..trailing {
            if i >= text.len() {
                break;
            }

            i = unicount::str_next(text, i);
        }

        debug_assert!(
            text.get(0..i).is_some(),
            "The UTF-8 offset {:?} ({:?} + {:?} characters) is not a valid UTF-8 boundary.",
            i,
            index,
            trailing
        );

        i
    }

    fn cursor_pos(&self, i: usize) -> [iface::Beam; 2] {
        let pango_layout = self.lock_layout();

        let (strong_pos, weak_pos) = pango_layout.get_cursor_pos(i as i32);

        use array::Array2;
        [strong_pos, weak_pos]
            .map(pango_rect_to_box2_f32)
            .map(|x| iface::Beam {
                x: x.min.x,
                top: x.min.y,
                bottom: x.max.y,
            })
    }

    fn num_lines(&self) -> usize {
        self.lock_layout().get_line_count() as usize
    }

    fn line_index_range(&self, i: usize) -> Range<usize> {
        let start = self.line_metrics[i].start_index as usize;
        let end = if let Some(line_metrics) = self.line_metrics.get(i + 1) {
            line_metrics.start_index as usize
        } else {
            self.text_len
        };
        start..end
    }

    fn line_vertical_bounds(&self, i: usize) -> Range<f32> {
        self.line_metrics[i].logical_extents.clone()
    }

    fn line_baseline(&self, i: usize) -> f32 {
        self.line_metrics[i].baseline
    }

    fn run_metrics_of_range(&self, range: Range<usize>) -> Vec<iface::RunMetrics> {
        debug_assert_ne!(range.start, range.end, "The range mustn't be empty");

        let line = self.line_from_index(range.start);
        debug_assert_eq!(
            self.line_from_index(range.end - 1),
            line,
            "The range mustn't span across lines"
        );

        let pango_layout = self.lock_layout();
        let mut iter = pango_layout.get_iter().unwrap();

        // Move to the line
        for _ in 0..line {
            iter.next_line();
        }

        let mut out_run_metrics = Vec::new();
        let mut pen = 0;
        let mut max_run_index = 0;

        //  `Layout::get_text` is actually very unsafe - The returned string
        // gets freed when `set_text` is called. We know this usage is safe
        // because (1) the returned `GString` doesn't outlive `pango_layout`,
        // and (2) we don't call `set_text` here.
        // Also, `set_text` validates UTF-8 validity, but simply replaces
        // invalid bytes with `0xff`, which is still invalid in UTF-8.
        // Thus, converting `GString` to `str` is actually unsafe. We know this
        // `as_str` is safe because we always pass a valid string to `set_text`.
        let text = pango_layout.get_text().unwrap();
        let text = text.as_str();

        // For each run in the line...
        pango_for_each_run_in_line(&mut iter, |run| {
            let pango_item = run.item();
            let mut pango_glyph_string = run.glyph_string();
            let pango_analysis = pango_item.analysis();

            let run_range = pango_item.offset() as usize
                ..pango_item.offset() as usize + pango_item.length() as usize;

            log::trace!("run_range = {:?}", run_range);

            // Update `max_run_index` to use later
            max_run_index = max_run_index.max(run_range.end);

            let run_left = pen;
            let run_width = pango_glyph_string.get_width();
            pen += run_width;
            let run_right = pen;

            log::trace!("  run_range = {:?}", run_left..run_right);

            if range.start >= run_range.end || range.end <= run_range.start {
                // This run doesn't overlap with `range`
                return;
            }

            // Make `RunFlags`
            let mut flags = iface::RunFlags::empty();
            let is_run_rtl = pango_analysis.level() % 2 != 0;
            if is_run_rtl {
                flags |= iface::RunFlags::RIGHT_TO_LEFT;
            }

            if range.start <= run_range.start && range.end >= run_range.end {
                // This run is completely inside `range`
                out_run_metrics.push(iface::RunMetrics {
                    flags,
                    index: run_range.clone(),
                    bounds: pango_coord_to_f32(run_left)..pango_coord_to_f32(run_right),
                });
                return;
            }

            // A note for complexity analysis:
            // The rest of this block will run only up to twice during
            // a single call to `run_metrics_of_range`

            // Clip the current run by `range`
            let out_start = range.start.max(run_range.start);
            let out_end = range.end.min(run_range.end);

            // Find the coordinates of the clipped endpoints.
            let run_text = &text[run_range.start..];
            let mut x1 = pango_glyph_string.index_to_x_2(
                run_text,
                &pango_analysis,
                (out_start - run_range.start) as i32,
                false,
            );
            let mut x2 = if out_end == run_range.end {
                if is_run_rtl {
                    0
                } else {
                    run_width
                }
            } else {
                pango_glyph_string.index_to_x_2(
                    run_text,
                    &pango_analysis,
                    (out_end - run_range.start) as i32,
                    false,
                )
            };

            log::trace!("  x1 (i = {:?}) = {:?}", out_start - run_range.start, x1);
            log::trace!("  x2 (i = {:?}) = {:?}", out_end - run_range.start, x2);

            if is_run_rtl {
                std::mem::swap(&mut x1, &mut x2);
            }

            debug_assert!(x1 <= x2);

            out_run_metrics.push(iface::RunMetrics {
                flags,
                index: out_start..out_end,
                bounds: pango_coord_to_f32(x1 + run_left)..pango_coord_to_f32(x2 + run_left),
            });
        });

        if range.end > max_run_index {
            // Pango doesn't generate runs for trailing newline characters, but
            // `TextLayout`'s API contract requires those.
            let x = pango_coord_to_f32(pen);
            out_run_metrics.push(iface::RunMetrics {
                flags: iface::RunFlags::empty(),
                index: max_run_index.max(range.start)..range.end,
                bounds: x..x,
            });
        }

        out_run_metrics
    }

    fn next_char(&self, i: usize, forward: bool) -> usize {
        self.next_char_with_log_attr(
            i,
            forward,
            flags![LogAttrFlags::{MANDATORY_BREAK | CURSOR_POSITION | WORD_START | WORD_END}],
        )
    }

    fn next_word(&self, i: usize, forward: bool) -> usize {
        self.next_char_with_log_attr(
            i,
            forward,
            if forward {
                flags![LogAttrFlags::{MANDATORY_BREAK | WORD_END}]
            } else {
                flags![LogAttrFlags::{MANDATORY_BREAK | WORD_START}]
            },
        )
    }
}

impl TextLayout {
    /// Find the first character after or before the given UTF-8 offset having
    /// `LogAttrFlags` intersecting with `flag`.
    fn next_char_with_log_attr(&self, mut i: usize, forward: bool, flag: LogAttrFlags) -> usize {
        let layout = self.lock_layout();

        // Get the analysis data. The last element corresponds to the
        // one-past-end position and is not needed here.
        let log_attrs = layout.get_log_attrs_readonly().split_last().unwrap().1;

        // First, we get the source string. These methods are actually supposed
        // to be `unsafe` (see the discussion in `run_metrics_of_range`).
        let text = layout.get_text().unwrap();
        let text = text.as_str();

        if forward {
            if i >= text.len() {
                return i;
            }
        } else {
            if i == 0 {
                return i;
            }
        }

        // Each element in `log_attrs` corresponds to a Unicode scalar.
        // So, we need to convert `i` to a number of Unicode scalars.
        let mut i_chars = num_scalars_in_utf8_str(&text.as_bytes()[0..i]);

        if forward {
            while {
                i = str_next(text, i);
                i_chars += 1;

                i_chars < log_attrs.len() && !log_attrs[i_chars].intersects(flag)
            } {}
        } else {
            while {
                i = str_prev(text, i);
                i_chars = i_chars.wrapping_sub(1);

                i_chars < log_attrs.len() && !log_attrs[i_chars].intersects(flag)
            } {}
        }

        i
    }
}

fn pango_for_each_run_in_line(iter: &mut pango::LayoutIter, mut f: impl FnMut(pango::LayoutRun)) {
    while let Some(run) = iter.get_run_readonly() {
        f(run);
        iter.next_run();
    }
}

#[inline]
fn pango_coord_to_f32(x: i32) -> f32 {
    x as f32 / pango::SCALE as f32
}

fn pango_rect_to_box2_f32(x: pango::Rectangle) -> Box2<f32> {
    let scale = pango::SCALE as f32;
    box2! {
        top_left: [x.x as f32 / scale, x.y as f32 / scale],
        size: [x.width as f32 / scale, x.height as f32 / scale],
    }
}

fn point_to_pango_xy(x: Point2<f32>) -> [i32; 2] {
    let scale = pango::SCALE as f32;
    [(x.x * scale) as i32, (x.y * scale) as i32]
}

trait LayoutExt {
    fn get_log_attrs_readonly(&self) -> &[LogAttrFlags];
}

impl LayoutExt for Layout {
    fn get_log_attrs_readonly(&self) -> &[LogAttrFlags] {
        use glib::translate::ToGlibPtr;
        unsafe {
            let mut count = MaybeUninit::uninit();
            let attrs = pango_sys::pango_layout_get_log_attrs_readonly(
                self.to_glib_full(),
                count.as_mut_ptr(),
            );

            let count = count.assume_init().try_into().expect("integer overflow");
            debug_assert_ne!(count, 0);

            std::slice::from_raw_parts(attrs as *const LogAttrFlags, count)
        }
    }
}

trait LayoutLineExt {
    fn start_index(&self) -> i32;
}

impl LayoutLineExt for LayoutLine {
    fn start_index(&self) -> i32 {
        use glib::translate::ToGlibPtr;
        unsafe { &*self.to_glib_full() }.start_index
    }
}

trait GlyphStringExt {
    /// The same as `pango::GlyphString::index_to_x` except that it takes
    /// `&Analysis` instead of `&mut Analysis`.
    fn index_to_x_2(
        &mut self,
        text: &str,
        analysis: &pango::Analysis,
        index_: i32,
        trailing: bool,
    ) -> i32;
}

impl GlyphStringExt for pango::GlyphString {
    fn index_to_x_2(
        &mut self,
        text: &str,
        analysis: &pango::Analysis,
        index_: i32,
        trailing: bool,
    ) -> i32 {
        use glib::translate::{ToGlib, ToGlibPtr, ToGlibPtrMut};
        use std::mem;
        // We can't use `GlyphString::index_to_x` because it takes
        // `&mut Analysis` as a parameter even though it doesn't actually
        // mutate the `Analysis`, whereas we only get `&Analysis`.
        // Transmuting `&_` to `&mut _` will break the pointer aliasing
        // rules, so we are definitely not doing that. Instead, we call the
        // underlying function, `pango_glyph_string_x_to_index` directly.
        let length = text.len() as i32;
        unsafe {
            let mut x_pos = mem::MaybeUninit::uninit();
            pango_sys::pango_glyph_string_index_to_x(
                self.to_glib_none_mut().0,
                text.to_glib_none().0,
                length,
                analysis.to_glib_none().0 as _,
                index_,
                trailing.to_glib(),
                x_pos.as_mut_ptr(),
            );
            x_pos.assume_init()
        }
    }
}

#[cfg(target_endian = "little")]
const fn bitfield_flag(i: u32) -> c_uint {
    1 << i
}

#[cfg(target_endian = "big")]
const fn bitfield_flag(i: u32) -> c_uint {
    const C_UINT_BITS: u32 = (std::mem::size_of::<c_uint>() / 8) as u32;
    1 << (C_UINT_BITS - 1 - i)
}

bitflags::bitflags! {
    /// `PangoLogAttr`
    ///
    /// GTK documentation:
    /// <https://developer.gnome.org/pango/stable/pango-Text-Processing.html#PangoLogAttr>
    ///
    /// gtk-rs/gir (automatic binding generator for gtk-rs) currently don't
    /// support structs with bitfields (https://github.com/gtk-rs/gir/issues/465)
    /// and translates them to incorrectly-sized `struct`s.
    struct LogAttrFlags: c_uint {
        const MANDATORY_BREAK = bitfield_flag(1);
        const CURSOR_POSITION = bitfield_flag(4);
        const WORD_START = bitfield_flag(5);
        const WORD_END = bitfield_flag(6);
    }
}

trait FontDescExt {
    /// The actually safe version of `FontDescription::set_family_static`.
    fn set_family_static_safe(&mut self, family: &'static CStr);
}

impl FontDescExt for FontDescription {
    fn set_family_static_safe(&mut self, family: &'static CStr) {
        use glib::translate::ToGlibPtrMut;
        unsafe {
            pango_sys::pango_font_description_set_family_static(
                self.to_glib_none_mut().0,
                family.as_ptr(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{CharStyle as _, TextLayout as _};

    /// Verifies the layout of `LogAttrFlags` by checking if `CURSOR_POSITION`
    /// is set for all expected cursor positions.
    #[test]
    fn log_attrs_cursor_position() {
        let _ = env_logger::builder().is_test(true).try_init();

        let char_style = CharStyle::new(CharStyleAttrs {
            sys: Some(iface::SysFontType::Normal),
            ..Default::default()
        });

        let layout = TextLayout::from_text("friendship", &char_style, None);
        let pango_layout = layout.lock_layout();
        let log_attrs = pango_layout.get_log_attrs_readonly();
        log::debug!(
            "attrs = {:#?}",
            log_attrs
                .iter()
                .map(|a| format!("{:032b}", a))
                .collect::<Vec<_>>()
        );

        assert!(log_attrs
            .iter()
            .all(|a| a.intersects(LogAttrFlags::CURSOR_POSITION)));
    }
}
