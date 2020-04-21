use alt_fp::FloatOrd;
use atom2::SetOnceAtom;
use cggeom::Box2;
use cgmath::Point2;
use directwrite::{
    enums::FontWeight,
    factory::Factory,
    text_layout::metrics::{HitTestMetrics, LineMetrics},
};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
    mem::MaybeUninit,
    ops::Range,
};
use utf16count::{
    find_utf16_pos, find_utf16_pos_in_utf8_str, rfind_utf16_pos_in_utf8_str, utf16_len_of_utf8_str,
};
use winapi::{
    shared::{minwindef::BYTE, winerror::S_OK},
    um::usp10,
};

use super::{
    codecvt::str_to_c_wstr,
    utils::{assert_hresult_ok, panic_hresult},
};
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
    pub(super) dwrite_layout: directwrite::TextLayout,
    pub(super) color: Option<iface::RGBAF32>,
    text: String,
    text_u16: Box<[u16]>,
    metrics: SetOnceAtom<Box<LayoutMetrics>>,
    break_analysis: SetOnceAtom<Box<BreakAnalysis>>,
}

struct LayoutMetrics {
    line_boundaries: Box<[LineBoundary]>,
    line_positions: Box<[f32]>,
    line_metrics_list: Vec<LineMetrics>,
}

struct BreakAnalysis {
    logattrs: Box<[usp10::SCRIPT_LOGATTR]>,
}

#[derive(Debug, Clone, Copy)]
struct LineBoundary {
    // Using `u32` here is a code size optimization - x86 doesn't have an
    // addressing mode for 16-byte elements.
    pos_u8: u32,
    pos_u16: u32,
}

impl fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextLayout")
            .field("dwrite_layout", &unsafe { self.dwrite_layout.get_raw() })
            .field("color", &self.color)
            .field("text", &self.text)
            .field("metrics", &self.metrics)
            .finish()
    }
}

impl fmt::Debug for LayoutMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayoutMetrics")
            .field("line_boundaries", &self.line_boundaries)
            .field("line_positions", &self.line_positions)
            .finish()
    }
}

impl fmt::Debug for BreakAnalysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BreakAnalysis").finish()
    }
}

impl TextLayout {
    fn ensure_metrics(&self) -> &LayoutMetrics {
        self.metrics
            .get_or_racy_insert_with(|| {
                let mut line_metrics_list = Vec::new();
                self.dwrite_layout.get_line_metrics(&mut line_metrics_list);

                debug_assert_ne!(line_metrics_list.len(), 0);

                let text = &self.text[..];

                // Generate `line_boundaries`
                let mut pos_u16 = 0;
                let mut pos_u8 = 0;
                let line_boundaries: Box<[LineBoundary]> = std::iter::once(LineBoundary {
                    pos_u8: 0,
                    pos_u16: 0,
                })
                .chain(line_metrics_list.iter().map(|line_metrics| {
                    let adv_u16 = line_metrics.length();
                    let adv_u8 = find_utf16_pos_in_utf8_str(
                        adv_u16 as usize,
                        &text.as_bytes()[pos_u8 as usize..],
                    )
                    .utf8_cursor as u32;
                    pos_u16 += adv_u16;
                    pos_u8 += adv_u8;
                    LineBoundary { pos_u8, pos_u16 }
                }))
                .collect::<Vec<_>>()
                // Converting to `Box<[LineBoundary]>` involves no reallocation
                // because this iterator is `ExactSizeIterator`
                .into();

                // generate `line_positions`
                let mut y = 0.0;
                let line_positions: Box<[f32]> = std::iter::once(0.0)
                    .chain(line_metrics_list.iter().map(|line_metrics| {
                        y += line_metrics.height();
                        y
                    }))
                    .collect::<Vec<_>>()
                    .into();

                let metrics = Box::new(LayoutMetrics {
                    line_metrics_list,
                    line_boundaries,
                    line_positions,
                });

                log::trace!("metrics({:?}) = {:?}", text, metrics);

                metrics
            })
            .0
    }

    fn ensure_break_analysis(&self) -> &BreakAnalysis {
        self.break_analysis
            .get_or_racy_insert_with(|| {
                let len = (self.text_u16.len() - 1)
                    .try_into()
                    .expect("string too long");

                // Specify to skip shaping and various extra processing.
                let script_analysis: usp10::SCRIPT_ANALYSIS = unsafe { std::mem::zeroed() };

                let logattrs = if len == 0 {
                    Box::new([])
                } else {
                    unsafe {
                        let mut logattrs = Vec::with_capacity(len as usize);
                        assert_hresult_ok(usp10::ScriptBreak(
                            self.text_u16.as_ptr(),
                            len,
                            &script_analysis,
                            logattrs.as_mut_ptr(),
                        ));
                        logattrs.set_len(len as usize);
                        logattrs.into_boxed_slice()
                    }
                };

                Box::new(BreakAnalysis { logattrs })
            })
            .0
    }

    /// Find the first character after or before the given UTF-8 offset having
    /// `SCRIPT_LOGATTR` intersecting with `flag`.
    fn next_char_with_logattr(&self, i: usize, forward: bool, flag: BYTE) -> usize {
        let text = self.text.as_bytes();

        // Get the analysis data.
        let logattrs = &self.ensure_break_analysis().logattrs[..];

        // Each element in `logattrs` corresponds to a UTF-16 unit;
        // So, we need to convert `i` to a UTF-16 offset.
        let i_chars = utf16_len_of_utf8_str(&text[0..i]);

        if forward {
            let chars = if i_chars >= logattrs.len() {
                0
            } else {
                logattrs[i_chars + 1..]
                    .iter()
                    .take_while(|a| (a.bit_fields & flag) == 0)
                    .count()
                    + 1
            };

            i + find_utf16_pos_in_utf8_str(chars, &text[i..]).utf8_cursor
        } else {
            let chars = if i_chars == 0 {
                0
            } else {
                logattrs[0..i_chars]
                    .iter()
                    .rev()
                    .take_while(|a| (a.bit_fields & flag) == 0)
                    .count()
                    + 1
            };

            rfind_utf16_pos_in_utf8_str(chars, &text[..i]).utf8_cursor
        }
    }
}

impl iface::TextLayout for TextLayout {
    type CharStyle = CharStyle;

    fn from_text(text: &str, style: &Self::CharStyle, width: Option<f32>) -> Self {
        assert!(u32::try_from(text.len()).is_ok(), "string too long");

        let text_u16 = str_to_c_wstr(text);

        let dwrite_layout = unsafe {
            let mut dwrite_layout = MaybeUninit::uninit();
            let width = width.unwrap_or(std::f32::INFINITY).fmax(0.0);
            let height = 0.0;
            assert_hresult_ok((&*G.dwrite.get_raw()).CreateTextLayout(
                text_u16.as_ptr(),
                (text_u16.len() - 1).try_into().expect("string too long"),
                style.to_dwrite_format().get_raw(),
                width,
                height,
                dwrite_layout.as_mut_ptr(),
            ));
            directwrite::TextLayout::from_raw(dwrite_layout.assume_init())
        };

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
            text: text.to_owned(),
            text_u16,
            metrics: SetOnceAtom::empty(),
            break_analysis: SetOnceAtom::empty(),
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

    fn cursor_index_from_point(&self, point: Point2<f32>) -> usize {
        let result = self.dwrite_layout.hit_test_point(point.x, point.y);
        let metrics = &result.metrics;

        // Find the text position of the edge closest to `point`
        let trailing = point.x - metrics.left() > metrics.width() * 0.5;
        let is_rtl = metrics.bidi_level() % 2 != 0;
        let pos = match (trailing, is_rtl) {
            (false, false) | (true, true) => metrics.text_position(),
            _ => metrics.text_position() + metrics.length(),
        };

        // Convert the text position to UTF-8
        find_utf16_pos(pos as usize, &self.text).utf8_cursor
    }

    fn cursor_pos(&self, i: usize) -> [iface::Beam; 2] {
        // Convert `i` to UTF-16
        let i = utf16_len_of_utf8_str(&self.text.as_bytes()[0..i]) as u32;

        let result1 = self
            .dwrite_layout
            .hit_test_text_position(i, true)
            .expect("HitTestTextPosition failed");

        let result2 = if i == 0 {
            result1
        } else {
            self.dwrite_layout
                .hit_test_text_position(i - 1, false)
                .expect("HitTestTextPosition failed")
        };

        use array::Array2;
        [result1, result2].map(|result| iface::Beam {
            x: result.point_x,
            top: result.metrics.top(),
            bottom: result.metrics.top() + result.metrics.height(),
        })
    }

    fn num_lines(&self) -> usize {
        self.dwrite_layout.get_line_metrics_count()
    }

    fn line_index_range(&self, i: usize) -> Range<usize> {
        let line_boundaries = &self.ensure_metrics().line_boundaries[..];

        debug_assert_eq!(line_boundaries.len(), self.num_lines() + 1);

        let the_boundaries = &line_boundaries[i..][..2];
        the_boundaries[0].pos_u8 as usize..the_boundaries[1].pos_u8 as usize
    }

    fn line_vertical_bounds(&self, i: usize) -> Range<f32> {
        let line_positions = &self.ensure_metrics().line_positions[..];

        debug_assert_eq!(line_positions.len(), self.num_lines() + 1);

        let the_positions = &line_positions[i..][..2];
        the_positions[0]..the_positions[1]
    }

    fn line_baseline(&self, i: usize) -> f32 {
        let metrics = self.ensure_metrics();
        metrics.line_metrics_list[i].baseline() * 1.1 + metrics.line_positions[i]
    }

    fn run_metrics_of_range(&self, range: Range<usize>) -> Vec<iface::RunMetrics> {
        debug_assert_ne!(range.start, range.end, "The range mustn't be empty");

        // Finding the containing line isn't strictly necessary, but allows up
        // to expedite the process of converting the range to UTF-16.
        // Without this, we can't meet the time complexity requirement of
        // this method (`O(line_len_8*log(line_len_8) +
        // line_len_16*log(line_len_16) + line_i)`).
        let line = self.line_from_index(range.start);
        debug_assert_eq!(
            self.line_from_index(range.end - 1),
            line,
            "The range mustn't span across lines"
        );

        // Convert `range` to UTF-16
        let metrics = self.ensure_metrics();
        let line_start = metrics.line_boundaries[line];
        let text = self.text.as_bytes();
        let start_u16 = utf16_len_of_utf8_str(&text[line_start.pos_u8 as usize..range.start])
            as u32
            + line_start.pos_u16;
        let len_u16 = utf16_len_of_utf8_str(&text[range.clone()]) as u32;

        log::trace!("run_metrics_of_range({:?})", range);
        log::trace!("  line = {:?}", line);
        log::trace!("  line_start = {:?}", line_start);
        log::trace!("  range_u16 = {:?}", start_u16..start_u16 + len_u16);

        // DirectWrite automatically rounds endpoints to the previous cluster,
        // which might turn the range into an empty one. The precondition of
        // this method does not allow an empty range, so we don't want the range
        // to be empty even after rounding. To avoid this, we manually round
        // `start_u16 + len_u16` to the next cluster boundary.
        let len_u16 = {
            let htm = self
                .dwrite_layout
                .hit_test_text_position(start_u16 + len_u16, true)
                .unwrap()
                .metrics;
            if htm.text_position() == start_u16 + len_u16 {
                len_u16
            } else {
                htm.text_position() + htm.length() - start_u16
            }
        };
        log::trace!(
            "  range_u16 (rounded) = {:?}",
            start_u16..start_u16 + len_u16
        );

        // Retrieve a list of `HitTestMetrics`
        let mut hit_test_metrics_list = Vec::new();
        self.dwrite_layout.hit_test_text_range_2(
            start_u16,
            len_u16,
            0.0,
            0.0,
            &mut hit_test_metrics_list,
        );

        log::trace!(
            "  The endpoints of the returned runs (UTF-16) = {:?}",
            hit_test_metrics_list
                .iter()
                .map(|htm| htm.text_position()..htm.text_position() + htm.length())
                .collect::<Vec<_>>()
        );
        log::trace!(
            "  The extents of the returned runs (UTF-16) = {:?}",
            hit_test_metrics_list
                .iter()
                .map(|htm| htm.left()..htm.left() + htm.width())
                .collect::<Vec<_>>()
        );

        // Discard zero-sized runs, which we aren't interested in
        hit_test_metrics_list.retain(|htm| htm.length() > 0);

        // The earliest position in `hit_test_metrics_list` might differ from
        // `start_u16` if `start_u16` is in the middle of a grapheme cluster.
        let min_u16 = hit_test_metrics_list
            .iter()
            .map(|htm| htm.text_position())
            .min()
            .unwrap_or(start_u16);

        // Collect the endpoints. Sort them, which lets us compute their
        // mapping to UTF-8 offsets in `O(line_len_8)`.
        //
        // The ranges represented by `hit_test_metrics_list` are a partition
        // of `range` minus any trailing newline characters. This means that
        // if we add `htm.text_position() + htm.length()`, we are also adding
        // `htm2.text_position()` for the subsequent `htm2` unless `htm2`
        // doesn't exist.
        //
        //                     v htm.pos+htm.len     v htm2.pos+htm2.len
        //      ┌──────────────┬─────────────────────┐
        //      | htm          | htm2                |
        //      └──────────────┴─────────────────────┘
        //      ^ htm.pos      ^ htm2.pos
        //
        // However, `htm2.text_position()` isn't added for the earliest run
        // because there's no corresponding `htm`. This case is covered by
        // `std::iter::one((min_u16, 0))`.
        let mut eps: Vec<(u32, u32)> = std::iter::once((min_u16, 0))
            .chain(
                hit_test_metrics_list
                    .iter()
                    .map(|htm| (htm.text_position() + htm.length(), 0)),
            )
            .collect();
        minisort::minisort_by_key(&mut eps, |&(pos_u16, _)| pos_u16);

        eps.iter_mut().fold(
            (line_start.pos_u16, line_start.pos_u8),
            |(last_u16, last_u8), (pos_u16, pos_u8)| {
                *pos_u8 = find_utf16_pos_in_utf8_str(
                    (*pos_u16 - last_u16) as usize,
                    &text[last_u8 as usize..],
                )
                .utf8_cursor as u32
                    + last_u8;

                (*pos_u16, *pos_u8)
            },
        );
        log::trace!("  eps = {:?}", eps);

        // Convert the list of `HitTestMetrics`
        let mut out_run_metrics: Vec<iface::RunMetrics> = hit_test_metrics_list
            .iter()
            .map(|htm| {
                // Make `RunFlags`
                let mut flags = iface::RunFlags::empty();
                if htm.bidi_level() % 2 != 0 {
                    flags |= iface::RunFlags::RIGHT_TO_LEFT;
                }

                // Convert `htm.text_position()` to UTF-8
                // (`O(log(hit_test_metrics_list.len()))`)
                let i = eps
                    .binary_search_by_key(&htm.text_position(), |&(pos_u16, _)| pos_u16)
                    .unwrap();
                let index = eps[i].1 as usize..eps[i + 1].1 as usize;
                debug_assert_eq!(eps[i + 1].0, htm.text_position() + htm.length());

                let bounds = htm.left()..htm.left() + htm.width();

                iface::RunMetrics {
                    flags,
                    index,
                    bounds,
                }
            })
            .collect();

        let max_returned_index = eps
            .last()
            .map(|&(_, pos_u8)| pos_u8 as usize)
            .unwrap_or(range.start);
        if range.end > max_returned_index {
            // DirectWrite doesn't generate runs for trailing newline characters,
            // but `TextLayout`'s API contract requires those.
            log::trace!(
                "  Synthesizing the run for the suffix {:?}",
                max_returned_index.max(range.start)..range.end
            );
            let http = self
                .dwrite_layout
                .hit_test_text_position(metrics.line_boundaries[line + 1].pos_u16 as u32 - 1, true)
                .expect("HitTestTextPosition failed");
            let x = http.metrics.left();
            out_run_metrics.push(iface::RunMetrics {
                flags: iface::RunFlags::empty(),
                index: max_returned_index.max(range.start)..range.end,
                bounds: x..x,
            });
        }

        debug_assert_ne!(out_run_metrics.len(), 0);

        out_run_metrics
    }

    fn next_char(&self, i: usize, forward: bool) -> usize {
        self.next_char_with_logattr(i, forward, SCRIPT_LOGATTR_CHAR_STOP)
    }

    fn next_word(&self, i: usize, forward: bool) -> usize {
        self.next_char_with_logattr(i, forward, SCRIPT_LOGATTR_WORD_STOP)
    }
}

const SCRIPT_LOGATTR_CHAR_STOP: BYTE = 1 << 2;
const SCRIPT_LOGATTR_WORD_STOP: BYTE = 1 << 3;

trait TextLayoutExt {
    fn hit_test_text_range_2(
        &self,
        position: u32,
        length: u32,
        origin_x: f32,
        origin_y: f32,
        metrics: &mut Vec<HitTestMetrics>,
    );
}

impl TextLayoutExt for directwrite::TextLayout {
    /// The fixed version of `hit_test_text_range` (back ported from the
    /// `master` version of `directwrite`).
    ///
    /// `hit_test_text_range` of `directwrite 0.1.4` has a bug caused by
    /// interpreting `E_NOT_SUFFICIENT_BUFFER` as an error, making it
    /// practically unusable.
    fn hit_test_text_range_2(
        &self,
        position: u32,
        length: u32,
        origin_x: f32,
        origin_y: f32,
        metrics: &mut Vec<HitTestMetrics>,
    ) {
        use std::ptr;
        const E_NOT_SUFFICIENT_BUFFER: i32 = -2147024774;
        metrics.clear();

        unsafe {
            let ptr = &*self.get_raw();

            // Calculate the total number of items we need
            let mut actual_count = 0;
            let res = ptr.HitTestTextRange(
                position,
                length,
                origin_x,
                origin_y,
                ptr::null_mut(),
                0,
                &mut actual_count,
            );
            match res {
                E_NOT_SUFFICIENT_BUFFER => (),
                S_OK => return,
                hr => panic_hresult(hr),
            }

            metrics.reserve(actual_count as usize);
            let buf_ptr = metrics[..].as_mut_ptr() as *mut _;
            let len = metrics.capacity() as u32;
            let res = ptr.HitTestTextRange(
                position,
                length,
                origin_x,
                origin_y,
                buf_ptr,
                len,
                &mut actual_count,
            );
            if res != S_OK {
                panic_hresult(res);
            }

            metrics.set_len(actual_count as usize);
        }
    }
}
