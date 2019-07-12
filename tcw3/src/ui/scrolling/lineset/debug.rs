use itertools::{unfold, Itertools};
use std::{fmt, ops::Range};

use super::{Index, Lineset, Size};

enum DebugItem {
    LineGr {
        index_range: Range<Index>,
        pos_range: Range<Size>,
    },
    LodGrStart {
        index: Index,
        lod: u8,
    },
}

impl fmt::Debug for DebugItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DebugItem::LineGr {
                index_range,
                pos_range,
            } => write!(
                f,
                "LineGr {{ index: {:8?}, pos: {:8?} }}",
                index_range, pos_range
            ),
            DebugItem::LodGrStart { index, lod } => {
                write!(f, "LodGr  {{ index: {:8?}.., lod: {:?} }}", index, lod)
            }
        }
    }
}

impl DebugItem {
    fn index(&self) -> Index {
        match self {
            DebugItem::LineGr { index_range, .. } => index_range.start,
            DebugItem::LodGrStart { index, .. } => *index,
        }
    }
}

impl fmt::Debug for Lineset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let line_grs = unfold((self.line_grs.iter(), 0, 0), |(line_gr_it, index, pos)| {
            line_gr_it.next().map(|line_gr| {
                let last_index = *index;
                let last_pos = *pos;
                *index += line_gr.num_lines;
                *pos += line_gr.size;
                DebugItem::LineGr {
                    index_range: last_index..*index,
                    pos_range: last_pos..*pos,
                }
            })
        });

        let lod_grs = self.lod_grs.iter().map(|lod_gr| DebugItem::LodGrStart {
            index: lod_gr.index,
            lod: lod_gr.lod,
        });

        f.debug_list()
            .entries(line_grs.merge_by(lod_grs, |a, b| a.index() < b.index()))
            .finish()
    }
}
