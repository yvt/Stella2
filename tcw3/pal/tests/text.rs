#![feature(is_sorted)]
use cggeom::prelude::*;
use itertools::iproduct;
use std::{iter::once, ops::Range};
use tcw3_pal::{self as pal, prelude::*};
use unicode_segmentation::UnicodeSegmentation;

mod common;

#[test]
fn test_text_layout_invariants() {
    common::try_init_logger_for_default_harness();

    let patterns = [
        "",
        "\r",
        "\r\r",
        "книга",
        "good apple cider",
        "✨🦄✨",
        "🇳🇮",
        // TODO: "🇳🇮\r",
        "book - كِتَاب‎",
        " 'book' translates to 'كِتَاب‎'.",
        " 'book' translates \r to 'كِتَاب‎'.",
    ];

    let char_style = pal::CharStyle::new(pal::CharStyleAttrs {
        ..Default::default()
    });

    for text in patterns.iter() {
        log::info!("{:?}", text);

        // TODO: try the right-to-left primary directionality

        let text_layout = pal::TextLayout::from_text(text, &char_style, None);
        log::debug!("  text_layout = {:?}", text_layout);

        let visual_bounds = text_layout.visual_bounds();
        log::debug!("  visual_bounds = {:?}", visual_bounds.display_im());

        let layout_bounds = text_layout.layout_bounds();
        log::debug!("  layout_bounds = {:?}", layout_bounds.display_im());

        // The bounding boxes must be valid (i.e., mustn't have a negative size)
        assert!(visual_bounds.is_valid());
        assert!(layout_bounds.is_valid());

        // `cursor_pos` must succeed for all positions
        // (Note: The result is not necessarily monotonic)
        log::debug!("  Sweeping `cursor_pos`");
        for i in text
            .char_indices()
            .map(|(i, _char)| i)
            .chain(once(text.len()))
        {
            let pos = text_layout.cursor_pos(i);
            log::trace!("    cursor_pos({:?}) = {:?}", i, pos);
        }

        let line_ranges: Vec<_> = (0..text_layout.num_lines())
            .map(|i| text_layout.line_index_range(i))
            .collect();
        log::debug!("  line_ranges = {:?}", line_ranges);

        // There must be at least one line
        assert_ne!(text_layout.num_lines(), 0);

        // `line_ranges` must monotonically increase
        assert!(line_ranges.iter().map(|i| i.start).is_sorted());

        // `line_ranges` must be a partition of the source string
        assert!(line_ranges.windows(2).all(|r| r[0].end == r[1].start));
        assert_eq!(line_ranges.last().unwrap().end, text.len());
        assert_eq!(line_ranges.first().unwrap().start, 0);

        // `line_from_index` must be the inverse mapping
        for (line, line_range) in line_ranges.iter().enumerate() {
            for i in line_range.clone() {
                assert!(
                    text_layout.line_from_index(i) == line,
                    "text_layout.line_from_index({:?}) != {:?}",
                    i,
                    line
                );
            }
        }

        assert!(text_layout.line_from_index(text.len()) == line_ranges.len() - 1);

        for (line_i, line_range) in line_ranges.iter().cloned().enumerate() {
            log::info!("  line[{:?}] = {:?}", line_i, &text[line_range.clone()]);

            if line_range.len() == 0 {
                continue;
            }

            // Exclude the trailing newline character from the `cursor_pos` test
            // because the width of such a character is inconsistent between
            // platforms and even OS versions
            let mut line_textual_range = line_range.clone();
            let last_char = text.as_bytes()[line_range.end - 1];
            if matches!(last_char, 13 | 10) {
                log::info!("  Trimming the trailing newline character in `line_textual_range`");
                line_textual_range.end -= 1;
            }

            let line_valid_indices: Vec<usize> = text[line_range.clone()]
                .char_indices()
                .map(|(i, _char)| i + line_range.start)
                .chain(once(line_range.end))
                .collect();

            let line_grapheme_boundary_indices: Vec<usize> = text[line_range.clone()]
                .grapheme_indices(false)
                .map(|(i, _char)| i + line_range.start)
                .chain(once(line_range.end))
                .collect();

            log::debug!("    line_valid_indices = {:?}", line_valid_indices);
            log::debug!(
                "    line_grapheme_boundary_indices = {:?}",
                line_grapheme_boundary_indices
            );

            let run_metrics = text_layout.run_metrics_of_range(line_range.clone());
            log::debug!("    runs({:?}) = {:?}", line_range, run_metrics);

            // `RunMetrics::index` must be a partition of `line_range`
            let mut run_ranges: Vec<_> = run_metrics.iter().map(|m| m.index.clone()).collect();
            run_ranges.sort_by_key(|r| r.start);
            assert_eq!(run_ranges.first().unwrap().start, line_range.start);
            assert_eq!(run_ranges.last().unwrap().end, line_range.end);
            assert!(run_ranges.windows(2).all(|r| r[0].end == r[1].start));

            if line_textual_range.len() > 0 {
                let run_metrics = text_layout.run_metrics_of_range(line_textual_range.clone());
                log::debug!("    runs({:?}) = {:?}", line_textual_range, run_metrics);

                // Each `RunMetrics` must be consistent with `cursor_pos`
                // (Grapheme cluster rounding might be inconsistent, so this test
                // can't be done for substrings)
                for rm in run_metrics.iter() {
                    let is_rtl = rm.flags.contains(pal::RunFlags::RIGHT_TO_LEFT);

                    let mut rm_range = [rm.index.start, rm.index.end];
                    while rm_range[1] > rm_range[0]
                        && matches!(text.as_bytes()[rm_range[1] - 1], 13 | 10)
                    {
                        rm_range[1] -= 1;
                    }

                    let epsilon = 0.1;

                    // The left edge
                    let expected = text_layout.cursor_pos(rm_range[is_rtl as usize]);
                    assert!(
                        (rm.bounds.start - expected[0].x).abs() < epsilon
                            || (rm.bounds.start - expected[1].x).abs() < epsilon,
                        "rm.bounds.start ({:?}) doesn't align with neither of {:?}",
                        rm.bounds.start,
                        expected
                    );

                    // The right edge
                    let expected = text_layout.cursor_pos(rm_range[!is_rtl as usize]);
                    assert!(
                        (rm.bounds.end - expected[0].x).abs() < epsilon
                            || (rm.bounds.end - expected[1].x).abs() < epsilon,
                        "rm.bounds.end ({:?}) doesn't align with neither of {:?}",
                        rm.bounds.end,
                        expected
                    );
                }
            }

            // For every possible range in the line...
            for (i1, i2) in iproduct!(0..line_valid_indices.len(), 0..line_valid_indices.len())
                .filter(|(i1, i2)| i1 < i2)
            {
                let subrange = line_valid_indices[i1]..line_valid_indices[i2];
                let run_metrics = text_layout.run_metrics_of_range(subrange.clone());

                log::trace!("    runs({:?}) = {:?}", subrange, run_metrics);

                // The union of `RunMetrics::bounds` must be monotonic with
                // reference to the logical range. To put it simply, if you
                // select a narrower range, the selection rectangle should be
                // narrower.
                if i2 > i1 + 1 {
                    let subrange = line_valid_indices[i1 + 1]..line_valid_indices[i2];
                    let run_metrics1 = text_layout.run_metrics_of_range(subrange.clone());

                    let subrange = line_valid_indices[i1]..line_valid_indices[i2 - 1];
                    let run_metrics2 = text_layout.run_metrics_of_range(subrange.clone());

                    assert!(
                        is_disjoint_ranges_subset_of(
                            run_metrics1.iter().map(|m| m.bounds.clone()),
                            run_metrics.iter().map(|m| inflate_range(&m.bounds)),
                        ),
                        "The union of bounds of {:?} is not a subset of that of {:?}.",
                        run_metrics1,
                        run_metrics,
                    );
                    assert!(
                        is_disjoint_ranges_subset_of(
                            run_metrics2.iter().map(|m| m.bounds.clone()),
                            run_metrics.iter().map(|m| inflate_range(&m.bounds)),
                        ),
                        "The union of bounds of {:?} is not a subset of that of {:?}.",
                        run_metrics2,
                        run_metrics,
                    );
                }
            }

            // `cursor_index_from_point`
            let mut i = 0;
            while i < line_grapheme_boundary_indices.len() {
                let beams = text_layout.cursor_pos(line_grapheme_boundary_indices[i]);

                // Find the range of positions possibly confused with
                // `line_grapheme_boundary_indices[i]`
                let mut i_end = i + 1;
                while i_end < line_grapheme_boundary_indices.len() {
                    let beams2 = text_layout.cursor_pos(line_grapheme_boundary_indices[i]);
                    let near = iproduct!(beams.iter(), beams2.iter())
                        .any(|(b1, b2)| (b1.x - b2.x).abs() < 0.2);
                    if !near {
                        break;
                    }
                    i_end += 1;
                }

                let i_range =
                    line_grapheme_boundary_indices[i]..=line_grapheme_boundary_indices[i_end - 1];

                let y = (beams[0].top + beams[0].bottom) / 2.0;
                let got0 = text_layout.cursor_index_from_point([beams[0].x, y].into());
                let got1 = text_layout.cursor_index_from_point([beams[1].x, y].into());
                assert!(
                    i_range.contains(&got0) || i_range.contains(&got1),
                    "{:?} ∉ {:?} (beams[0].x = {:?}) && {:?} ∉ {:?} (beams[1].x = {:?})",
                    got0,
                    i_range,
                    beams[0].x,
                    got1,
                    i_range,
                    beams[1].x,
                );

                i = i_end;
            }
        } // line_ranges.iter().enumerate()

        // The set of boundaries defined by `next_char` must be consistent for
        // all invocations to `next_char` with the same input string
        let mut is_char_boundary: Vec<bool> = (0..=text.len()).map(|_| false).collect();
        {
            let mut i = 0;
            while i < text.len() {
                is_char_boundary[i] = true;
                let next_i = text_layout.next_char(i, true);
                assert!(next_i > i);
                i = next_i;
            }
            is_char_boundary[i] = true;
        }
        log::debug!("  is_char_boundary = {:?}", is_char_boundary);

        for (i, _) in text.char_indices() {
            let next_i = text_layout.next_char(i, true); // forward
            log::trace!("    next_char{:?} = {:?}", (i, true), next_i);
            assert!(next_i > i);

            // `next_i` must be the next boundary
            assert!(is_char_boundary[next_i]);
            assert!(is_char_boundary[i + 1..next_i].iter().all(|b| !b));
        }

        for (i, s) in text.char_indices() {
            let i = i + s.len_utf8();
            let next_i = text_layout.next_char(i, false); // backward
            log::trace!("    next_char{:?} = {:?}", (i, false), next_i);
            assert!(next_i < i);

            // `next_i` must be the previous boundary
            assert!(is_char_boundary[next_i]);
            assert!(is_char_boundary[next_i + 1..i].iter().all(|b| !b));
        }

        // `next_char` stops at the endpoints
        assert_eq!(text_layout.next_char(0, false), 0);
        assert_eq!(text_layout.next_char(text.len(), true), text.len());

        // The set of boundaries defined by `next_word` must be consistent for
        // all invocations to `next_word` with the same input string and the
        // same value of `forward`
        let mut next_boundary = 0;
        for (i, _) in text.char_indices() {
            let next_i = text_layout.next_word(i, true); // forward
            log::trace!("    next_word{:?} = {:?}", (i, true), next_i);
            assert!(next_i > i);

            if i == next_boundary {
                next_boundary = next_i;
            } else {
                assert_eq!(next_boundary, next_i);
            }
        }
        assert_eq!(next_boundary, text.len());

        next_boundary = text.len();
        for (i, s) in text.char_indices().rev() {
            let i = i + s.len_utf8();
            let next_i = text_layout.next_word(i, false); // backward
            log::trace!("    next_word{:?} = {:?}", (i, false), next_i);
            assert!(next_i < i);

            if i == next_boundary {
                next_boundary = next_i;
            } else {
                assert_eq!(next_boundary, next_i);
            }
        }
        assert_eq!(next_boundary, 0);

        // `next_word` stops at the endpoints
        assert_eq!(text_layout.next_word(0, false), 0);
        assert_eq!(text_layout.next_word(text.len(), true), text.len());
    }
}

fn inflate_range(x: &Range<f32>) -> Range<f32> {
    x.start - 0.1..x.end + 0.1
}

fn is_disjoint_ranges_subset_of<T: PartialOrd + std::fmt::Debug>(
    x: impl IntoIterator<Item = Range<T>>,
    y: impl IntoIterator<Item = Range<T>>,
) -> bool {
    use arrayvec::ArrayVec;
    let mut endpoints: Vec<(T, isize)> = y
        .into_iter()
        .map(|range| ArrayVec::from([(range.start, 1), (range.end, -1)]))
        .flatten()
        .collect();
    endpoints.sort_by(|x, y| x.partial_cmp(y).unwrap());

    let mut depth = 0;
    for (_, delta) in endpoints.iter_mut() {
        depth += *delta;
        *delta = depth;
    }

    // Remove zero-sized exterior intervals
    endpoints.dedup_by(|a, b| {
        if a.0 == b.0 {
            // retain the latter element
            std::mem::swap(a, b);
            true
        } else {
            false
        }
    });

    // Remove interior endpoints
    endpoints.dedup_by(|a, b| (a.1 > 0) == (b.1 > 0));

    assert_eq!(
        endpoints.last().unwrap().1,
        0,
        "bad endpoints: {:?}",
        endpoints
    );

    x.into_iter().all(|range| {
        let i = match endpoints.binary_search_by(|probe| probe.0.partial_cmp(&range.start).unwrap())
        {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    return false;
                } else {
                    i - 1
                }
            }
        };

        endpoints[i].1 > 0 && endpoints[i + 1].0 >= range.end
    })
}
